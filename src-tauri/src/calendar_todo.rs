use super::*;

const INVALID: &str = "CALENDAR_TODO_INVALID";
const UNAVAILABLE: &str = "CALENDAR_TODO_UNAVAILABLE";
const GROUP_UNAVAILABLE: &str = "CALENDAR_TODO_GROUP_UNAVAILABLE";

#[derive(Deserialize)]
pub(crate) struct CalendarTodoDraft {
    uuid: String,
    title: String,
    note: String,
    group_uuid: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CalendarTodoResult {
    todo: Todo,
    created: bool,
}

#[tauri::command]
pub(crate) fn create_calendar_todo(
    draft: CalendarTodoDraft,
    app: AppHandle,
    database: State<'_, Database>,
) -> Result<CalendarTodoResult, String> {
    let result = {
        let mut connection = lock_database(&database)?;
        create_in_connection(&mut connection, draft)
    };
    refresh_badge_after_success(&app, &result);
    result
}

#[tauri::command]
pub(crate) fn resolve_calendar_todo(
    uuid: String,
    database: State<'_, Database>,
) -> Result<Option<CalendarTodoResult>, String> {
    let connection = lock_database(&database)?;
    resolve_in_connection(&connection, &uuid)
}

fn validate_uuid(uuid: &str) -> Result<(), String> {
    if Uuid::parse_str(uuid)
        .map(|value| value.to_string())
        .as_deref()
        != Ok(uuid)
    {
        return Err(INVALID.into());
    }
    Ok(())
}

fn valid_text(value: &str, multiline: bool) -> bool {
    value.chars().all(|ch| {
        !matches!(ch, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}'
            | '\u{7f}'..='\u{9f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            && (multiline || !matches!(ch, '\t' | '\r' | '\n' | '\u{2028}' | '\u{2029}'))
    })
}

fn resolve_in_connection(
    connection: &Connection,
    uuid: &str,
) -> Result<Option<CalendarTodoResult>, String> {
    validate_uuid(uuid)?;
    let terminal: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=?1)",
            [uuid],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if terminal {
        return Err(UNAVAILABLE.into());
    }
    let id: Option<i64> = connection
        .query_row("SELECT id FROM todos WHERE uuid=?1", [uuid], |row| {
            row.get(0)
        })
        .optional()
        .map_err(database_error)?;
    let Some(id) = id else { return Ok(None) };
    let todo = find_todo(connection, id)?.ok_or(UNAVAILABLE)?;
    if todo.deleted_at.is_some() || todo.archived_at.is_some() {
        return Err(UNAVAILABLE.into());
    }
    Ok(Some(CalendarTodoResult {
        todo,
        created: false,
    }))
}

fn create_in_connection(
    connection: &mut Connection,
    draft: CalendarTodoDraft,
) -> Result<CalendarTodoResult, String> {
    let transaction = connection.transaction().map_err(database_error)?;
    // Resolve before draft validation: a retry must not overwrite later user edits.
    if let Some(existing) = resolve_in_connection(&transaction, &draft.uuid)? {
        return Ok(existing);
    }
    if draft.title.trim().is_empty()
        || draft.title.trim().encode_utf16().count() > 100
        || draft.note.encode_utf16().count() > TODO_NOTE_MAX_CHARS
        || !valid_text(&draft.title, false)
        || !valid_text(&draft.note, true)
    {
        return Err(INVALID.into());
    }
    let group_uuid = match draft.group_uuid.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(group) => {
            validate_uuid(group)?;
            let exists: bool = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM groups WHERE uuid=?1 AND deleted_at IS NULL)",
                    [group],
                    |row| row.get(0),
                )
                .map_err(database_error)?;
            if !exists {
                return Err(GROUP_UNAVAILABLE.into());
            }
            Some(group.to_string())
        }
    };
    let todo =
        create_todo_with_uuid_in_connection(&transaction, &draft.title, group_uuid, &draft.uuid)?;
    transaction
        .execute(
            "UPDATE todos SET note=?1 WHERE id=?2",
            params![draft.note, todo.id],
        )
        .map_err(database_error)?;
    let todo = find_todo(&transaction, todo.id)?.ok_or(INVALID)?;
    transaction.commit().map_err(database_error)?;
    Ok(CalendarTodoResult {
        todo,
        created: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrate};

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        connection
    }

    fn draft(uuid: &str) -> CalendarTodoDraft {
        CalendarTodoDraft {
            uuid: uuid.into(),
            title: "  Meeting tomorrow  ".into(),
            note: "Meeting tomorrow\n2026-10-02 15:00 - 16:00 (Asia/Shanghai)\nRoom A".into(),
            group_uuid: None,
        }
    }

    #[test]
    fn calendar_todo_atomic_defaults_dirty_and_normal_sync_round_trip() {
        let mut db = connection();
        let uuid = Uuid::new_v4().to_string();
        let result = create_in_connection(&mut db, draft(&uuid)).unwrap();
        assert!(result.created);
        let todo = result.todo;
        assert_eq!(todo.uuid, uuid);
        assert_eq!(todo.title, "Meeting tomorrow");
        assert_eq!(todo.group_uuid, None);
        assert!(!todo.completed && !todo.pinned);
        assert_eq!(todo.priority, 0);
        assert_eq!(
            (todo.due_date, todo.due_at, todo.reminder_at),
            (None, None, None)
        );
        assert_eq!(
            (
                todo.repeat_rule,
                todo.repeat_next_due_date,
                todo.repeat_series_uuid
            ),
            (None, None, None)
        );
        let dirty: String = db
            .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(dirty.contains("todos"));
        let doc = sync::build_document(&db, now_millis()).unwrap();
        let wire = serde_json::to_string(&doc).unwrap();
        let copy: SyncDocument = serde_json::from_str(&wire).unwrap();
        sync::validate_document(&copy).unwrap();
        assert_eq!(copy.todos[0].note, todo.note);
        assert_eq!(copy.todos[0].uuid, uuid);
        assert!(!wire.contains("calendarId") && !wire.contains("owner_generation"));
        let mut peer = connection();
        sync::merge_remote_document(&mut peer, &copy, now_millis()).unwrap();
        let received = resolve_in_connection(&peer, &uuid).unwrap().unwrap().todo;
        assert_eq!(received.title, todo.title);
        assert_eq!(received.note, todo.note);
        assert_eq!(received.group_uuid, None);
        assert_eq!(
            (received.due_at, received.due_date, received.reminder_at),
            (None, None, None)
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM task_progress_entries", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }

    #[test]
    fn calendar_todo_lost_reply_retry_keeps_later_edits_and_never_inserts_twice() {
        let mut db = connection();
        let uuid = Uuid::new_v4().to_string();
        let first = create_in_connection(&mut db, draft(&uuid)).unwrap();
        db.execute(
            "UPDATE todos SET title='later edit', note='changed after save' WHERE uuid=?1",
            [&uuid],
        )
        .unwrap();
        let mut retry = draft(&uuid);
        retry.title.clear();
        retry.note = "x".repeat(1001);
        retry.group_uuid = Some(Uuid::new_v4().to_string());
        let resolved = create_in_connection(&mut db, retry).unwrap();
        assert!(!resolved.created);
        assert_eq!(resolved.todo.id, first.todo.id);
        assert_eq!(resolved.todo.title, "later edit");
        assert_eq!(resolved.todo.note.as_deref(), Some("changed after save"));
        assert_eq!(
            resolve_in_connection(&db, &uuid).unwrap().unwrap().todo,
            resolved.todo
        );
        assert_eq!(list_todos_from_connection(&db).unwrap().len(), 1);
    }

    #[test]
    fn calendar_todo_rejects_lifecycle_terminal_without_recreation() {
        for state in ["archive", "delete", "purge"] {
            let mut db = connection();
            let uuid = Uuid::new_v4().to_string();
            create_in_connection(&mut db, draft(&uuid)).unwrap();
            match state {
                "archive" => {
                    db.execute("UPDATE todos SET archived_at=1 WHERE uuid=?1", [&uuid])
                        .unwrap();
                }
                "delete" => {
                    db.execute("UPDATE todos SET deleted_at=1 WHERE uuid=?1", [&uuid])
                        .unwrap();
                }
                _ => {
                    db.execute("DELETE FROM todos WHERE uuid=?1", [&uuid])
                        .unwrap();
                    db.execute("INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES('todo',?1,?2,1)", params![uuid,Uuid::new_v4().to_string()]).unwrap();
                }
            }
            assert_eq!(resolve_in_connection(&db, &uuid).unwrap_err(), UNAVAILABLE);
            assert_eq!(
                create_in_connection(&mut db, draft(&uuid)).unwrap_err(),
                UNAVAILABLE
            );
        }
    }

    #[test]
    fn calendar_todo_failed_note_write_rolls_back_task_and_dirty_revision() {
        let mut db = connection();
        let before: String = db
            .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        db.execute_batch("CREATE TRIGGER calendar_note_failure BEFORE UPDATE OF note ON todos BEGIN SELECT RAISE(ABORT,'test failure'); END;").unwrap();
        let uuid = Uuid::new_v4().to_string();
        assert!(create_in_connection(&mut db, draft(&uuid)).is_err());
        assert!(resolve_in_connection(&db, &uuid).unwrap().is_none());
        assert!(list_todos_from_connection(&db).unwrap().is_empty());
        let after: String = db
            .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(before, after);
        db.execute_batch("DROP TRIGGER calendar_note_failure;")
            .unwrap();
        assert!(create_in_connection(&mut db, draft(&uuid)).unwrap().created);
    }

    #[test]
    fn calendar_todo_validates_utf16_before_any_write_and_can_save_new_drafts() {
        let mut db = connection();
        let uuid = Uuid::new_v4().to_string();
        let mut input = draft(&uuid);
        input.note = "😀".repeat(501);
        assert_eq!(create_in_connection(&mut db, input).unwrap_err(), INVALID);
        let mut input = draft(&uuid);
        input.note = "😀".repeat(500);
        assert!(create_in_connection(&mut db, input).unwrap().created);
        assert!(
            create_in_connection(&mut db, draft(&Uuid::new_v4().to_string()))
                .unwrap()
                .created
        );
        assert_eq!(list_todos_from_connection(&db).unwrap().len(), 2);
    }

    #[test]
    fn calendar_todo_validates_title_uuid_and_deleted_group_atomically() {
        let mut db = connection();
        let uuid = Uuid::new_v4().to_string();
        let mut input = draft(&uuid);
        input.title = "  \n ".into();
        assert_eq!(create_in_connection(&mut db, input).unwrap_err(), INVALID);
        assert_eq!(
            create_in_connection(&mut db, draft("bad uuid")).unwrap_err(),
            INVALID
        );
        let group = create_group_in_connection(&db, "Work").unwrap();
        let mut input = draft(&uuid);
        input.group_uuid = Some(group.uuid.clone());
        db.execute(
            "UPDATE groups SET deleted_at=1 WHERE uuid=?1",
            [&group.uuid],
        )
        .unwrap();
        assert_eq!(
            create_in_connection(&mut db, input).unwrap_err(),
            GROUP_UNAVAILABLE
        );
        assert!(list_todos_from_connection(&db).unwrap().is_empty());
        assert!(resolve_in_connection(&db, &uuid).unwrap().is_none());
        db.execute(
            "UPDATE groups SET deleted_at=NULL WHERE uuid=?1",
            [&group.uuid],
        )
        .unwrap();
        let mut input = draft(&uuid);
        input.group_uuid = Some(group.uuid.clone());
        assert_eq!(
            create_in_connection(&mut db, input)
                .unwrap()
                .todo
                .group_uuid,
            Some(group.uuid)
        );
    }

    #[test]
    fn calendar_todo_title_boundary_and_null_text_match_the_form() {
        let mut db = connection();
        for (title, note) in [
            ("x".repeat(101), "note".into()),
            ("😀".repeat(51), "note".into()),
            ("bad\0title".into(), "note".into()),
            ("title".into(), "bad\0note".into()),
            ("bad\n title".into(), "note".into()),
            ("title".into(), "bad\u{202e}note".into()),
        ] {
            let mut input = draft(&Uuid::new_v4().to_string());
            input.title = title;
            input.note = note;
            assert_eq!(create_in_connection(&mut db, input).unwrap_err(), INVALID);
        }
        assert!(list_todos_from_connection(&db).unwrap().is_empty());
        let mut input = draft(&Uuid::new_v4().to_string());
        input.title = "😀".repeat(50);
        assert!(create_in_connection(&mut db, input).unwrap().created);
    }

    #[test]
    fn calendar_todo_completion_delete_restore_and_archive_use_normal_lifecycle() {
        let mut db = connection();
        let uuid = Uuid::new_v4().to_string();
        let todo = create_in_connection(&mut db, draft(&uuid)).unwrap().todo;
        assert!(
            set_todo_completed_in_connection(&mut db, todo.id, true)
                .unwrap()
                .updated_todo
                .completed
        );
        assert!(!create_in_connection(&mut db, draft(&uuid)).unwrap().created);
        soft_delete_todo_in_connection(&mut db, todo.id, None).unwrap();
        assert_eq!(resolve_in_connection(&db, &uuid).unwrap_err(), UNAVAILABLE);
        let restored = restore_todo_in_connection(&mut db, todo.id).unwrap();
        assert_eq!(restored.note, todo.note);
        assert!(!create_in_connection(&mut db, draft(&uuid)).unwrap().created);
        assert_eq!(archive_completed_todos_in_connection(&db).unwrap(), 1);
        assert_eq!(resolve_in_connection(&db, &uuid).unwrap_err(), UNAVAILABLE);
    }

    #[test]
    #[ignore = "Requires isolated Harmony input/output from test-calendar-todo-cross-client.cjs"]
    fn calendar_todo_cross_client_exchange() {
        let input = std::env::var("EGGDONE_CALENDAR_TODO_INPUT").expect("isolated input path");
        let output = std::env::var("EGGDONE_CALENDAR_TODO_OUTPUT").expect("isolated output path");
        let incoming: SyncDocument =
            serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
        assert_eq!(incoming.todos.len(), 1);
        let harmony = &incoming.todos[0];
        let mut db = connection();
        sync::merge_remote_document(&mut db, &incoming, now_millis()).unwrap();
        let received = resolve_in_connection(&db, &harmony.uuid)
            .unwrap()
            .unwrap()
            .todo;
        assert_eq!(received.note, harmony.note);
        assert_eq!(received.title, harmony.title);
        assert_eq!(received.group_uuid, harmony.group_uuid);
        let mut draft = draft(&Uuid::new_v4().to_string());
        draft.title = "Desktop calendar follow-up".into();
        draft.note = "Independent desktop task\nFull original event range".into();
        draft.group_uuid = harmony.group_uuid.clone();
        create_in_connection(&mut db, draft).unwrap();
        let result = sync::build_document(&db, now_millis()).unwrap();
        assert_eq!(result.todos.len(), 2);
        for todo in &result.todos {
            assert!(!todo.completed && !todo.pinned);
            assert_eq!(
                (todo.due_at, todo.reminder_at, todo.priority),
                (None, None, 0)
            );
            assert!(todo.due_date.is_none() && todo.repeat_rule.is_none());
        }
        std::fs::write(output, serde_json::to_vec(&result).unwrap()).unwrap();
    }
}
