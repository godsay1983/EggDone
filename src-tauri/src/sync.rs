use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    db::device_id,
    schedule::{local_date_from_timestamp, timestamp_for_local_date},
};

pub(crate) const SYNC_FORMAT_VERSION: u32 = 1;
const TODO_NOTE_MAX_CHARS: usize = 1000;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SyncTodo {
    pub uuid: String,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub group_uuid: Option<String>,
    pub completed: bool,
    #[serde(default)]
    pub pinned: bool,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub deleted_at: Option<i64>,
    #[serde(default)]
    pub archived_at: Option<i64>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub due_at: Option<i64>,
    #[serde(default)]
    pub reminder_at: Option<i64>,
    #[serde(default)]
    pub repeat_rule: Option<String>,
    #[serde(default)]
    pub repeat_next_due_date: Option<String>,
    #[serde(default)]
    pub repeat_series_uuid: Option<String>,
    pub updated_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SyncGroup {
    pub uuid: String,
    pub name: String,
    pub color: String,
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
    pub updated_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SyncDocument {
    pub format_version: u32,
    pub device_id: String,
    pub generated_at: i64,
    #[serde(default)]
    pub groups: Vec<SyncGroup>,
    pub todos: Vec<SyncTodo>,
}

pub(crate) fn build_document(
    connection: &Connection,
    generated_at: i64,
) -> Result<SyncDocument, String> {
    let device_id = device_id(connection).map_err(database_error)?;
    let mut group_statement = connection
        .prepare(
            "
            SELECT uuid, name, color, sort_order, created_at, updated_at, deleted_at, updated_by
            FROM groups
            ORDER BY uuid ASC
            ",
        )
        .map_err(database_error)?;
    let groups = group_statement
        .query_map([], map_sync_group)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    let mut statement = connection
        .prepare(
            "
            SELECT uuid, title, group_uuid, completed, pinned, sort_order, created_at, updated_at,
                   completed_at, deleted_at, archived_at, due_date, due_at, reminder_at,
                   repeat_rule, repeat_next_due_date, repeat_series_uuid, note, updated_by
            FROM todos
            ORDER BY uuid ASC
            ",
        )
        .map_err(database_error)?;
    let todos = statement
        .query_map([], map_sync_todo)
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?
        .into_iter()
        .map(canonicalize_repeat_schedule)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SyncDocument {
        format_version: SYNC_FORMAT_VERSION,
        device_id,
        generated_at,
        groups,
        todos,
    })
}

pub(crate) fn merge_documents(
    local: &SyncDocument,
    remote: &SyncDocument,
    generated_at: i64,
) -> Result<SyncDocument, String> {
    validate_document(local)?;
    validate_document(remote)?;

    let mut merged_groups = BTreeMap::<String, SyncGroup>::new();
    for group in local.groups.iter().chain(&remote.groups) {
        merged_groups
            .entry(group.uuid.clone())
            .and_modify(|current| {
                if compare_groups(group, current).is_gt() {
                    *current = group.clone();
                }
            })
            .or_insert_with(|| group.clone());
    }

    let mut merged = BTreeMap::<String, SyncTodo>::new();
    for todo in local.todos.iter().chain(&remote.todos) {
        merged
            .entry(todo.uuid.clone())
            .and_modify(|current| {
                if compare_todos(todo, current).is_gt() {
                    *current = todo.clone();
                }
            })
            .or_insert_with(|| todo.clone());
    }

    let todos = collapse_duplicate_active_repeat_instances(
        merged.into_values().collect(),
        generated_at,
        &local.device_id,
    );

    Ok(SyncDocument {
        format_version: SYNC_FORMAT_VERSION,
        device_id: local.device_id.clone(),
        generated_at,
        groups: merged_groups.into_values().collect(),
        todos,
    })
}

pub(crate) fn merge_remote_document(
    connection: &mut Connection,
    remote: &SyncDocument,
    generated_at: i64,
) -> Result<SyncDocument, String> {
    let local = build_document(connection, generated_at)?;
    let merged = merge_documents(&local, remote, generated_at)?;
    let transaction = connection.transaction().map_err(database_error)?;

    for group in &merged.groups {
        transaction
            .execute(
                "
                INSERT INTO groups (
                    uuid, name, color, sort_order, created_at, updated_at, deleted_at, updated_by
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                ON CONFLICT(uuid) DO UPDATE SET
                    name = excluded.name,
                    color = excluded.color,
                    sort_order = excluded.sort_order,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at,
                    updated_by = excluded.updated_by
                ",
                params![
                    group.uuid,
                    group.name.trim(),
                    group.color,
                    group.sort_order,
                    group.created_at,
                    group.updated_at,
                    group.deleted_at,
                    group.updated_by,
                ],
            )
            .map_err(database_error)?;
    }

    for todo in &merged.todos {
        let existing_due_at = transaction
            .query_row(
                "SELECT due_at FROM todos WHERE uuid = ?1",
                params![todo.uuid],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(database_error)?
            .flatten();
        let (due_date, due_at) = schedule_for_local_storage(todo, existing_due_at)?;
        transaction
            .execute(
                "
                INSERT INTO todos (
                    uuid, title, group_uuid, completed, pinned, sort_order, created_at, updated_at,
                    completed_at, deleted_at, archived_at, due_date, due_at, reminder_at,
                    repeat_rule, repeat_next_due_date, repeat_series_uuid, note, updated_by
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
                ON CONFLICT(uuid) DO UPDATE SET
                    title = excluded.title,
                    group_uuid = excluded.group_uuid,
                    completed = excluded.completed,
                    pinned = excluded.pinned,
                    sort_order = excluded.sort_order,
                    created_at = excluded.created_at,
                    updated_at = excluded.updated_at,
                    completed_at = excluded.completed_at,
                    deleted_at = excluded.deleted_at,
                    archived_at = excluded.archived_at,
                    due_date = excluded.due_date,
                    due_at = excluded.due_at,
                    reminder_at = excluded.reminder_at,
                    repeat_rule = excluded.repeat_rule,
                    repeat_next_due_date = excluded.repeat_next_due_date,
                    repeat_series_uuid = excluded.repeat_series_uuid,
                    note = excluded.note,
                    updated_by = excluded.updated_by
                ",
                params![
                    todo.uuid,
                    todo.title.trim(),
                    todo.group_uuid,
                    todo.completed,
                    todo.pinned,
                    todo.sort_order,
                    todo.created_at,
                    todo.updated_at,
                    todo.completed_at,
                    todo.deleted_at,
                    todo.archived_at,
                    due_date,
                    due_at,
                    todo.reminder_at,
                    todo.repeat_rule,
                    todo.repeat_next_due_date,
                    todo.repeat_series_uuid,
                    todo.note,
                    todo.updated_by,
                ],
            )
            .map_err(database_error)?;
    }

    transaction.commit().map_err(database_error)?;
    Ok(merged)
}

pub(crate) fn validate_document(document: &SyncDocument) -> Result<(), String> {
    if document.format_version != SYNC_FORMAT_VERSION {
        return Err(format!("同步文件版本 {} 不受支持", document.format_version));
    }
    if Uuid::parse_str(&document.device_id).is_err() {
        return Err("同步文件 device_id 无效".to_string());
    }
    if document.generated_at < 0 {
        return Err("同步文件 generated_at 无效".to_string());
    }

    let mut group_uuids = HashSet::new();
    for group in &document.groups {
        if Uuid::parse_str(&group.uuid).is_err() {
            return Err(format!("同步分组 UUID 无效：{}", group.uuid));
        }
        if Uuid::parse_str(&group.updated_by).is_err() {
            return Err(format!("同步分组 updated_by 无效：{}", group.updated_by));
        }
        if !group_uuids.insert(&group.uuid) {
            return Err(format!("同步文件包含重复分组 UUID：{}", group.uuid));
        }
        if group.name.trim().is_empty() {
            return Err("同步文件包含空分组名称".to_string());
        }
        if group.created_at < 0
            || group.updated_at < 0
            || group.deleted_at.is_some_and(|value| value < 0)
        {
            return Err("同步文件包含无效分组时间戳".to_string());
        }
    }

    let mut uuids = HashSet::new();
    for todo in &document.todos {
        if Uuid::parse_str(&todo.uuid).is_err() {
            return Err(format!("同步任务 UUID 无效：{}", todo.uuid));
        }
        if Uuid::parse_str(&todo.updated_by).is_err() {
            return Err(format!("同步任务 updated_by 无效：{}", todo.updated_by));
        }
        if todo
            .group_uuid
            .as_deref()
            .is_some_and(|value| Uuid::parse_str(value).is_err())
        {
            return Err(format!(
                "同步任务分组 UUID 无效：{}",
                todo.group_uuid.as_deref().unwrap_or_default()
            ));
        }
        if !uuids.insert(&todo.uuid) {
            return Err(format!("同步文件包含重复 UUID：{}", todo.uuid));
        }
        if todo
            .repeat_series_uuid
            .as_deref()
            .is_some_and(|value| Uuid::parse_str(value).is_err())
        {
            return Err(format!(
                "同步任务重复系列 UUID 无效：{}",
                todo.repeat_series_uuid.as_deref().unwrap_or_default()
            ));
        }
        if todo.title.trim().is_empty() {
            return Err("同步文件包含空标题任务".to_string());
        }
        if todo
            .note
            .as_deref()
            .is_some_and(|value| value.chars().count() > TODO_NOTE_MAX_CHARS)
        {
            return Err(format!(
                "同步文件包含超过 {TODO_NOTE_MAX_CHARS} 个字符的备注"
            ));
        }
        if todo.created_at < 0
            || todo.updated_at < 0
            || todo.completed_at.is_some_and(|value| value < 0)
            || todo.deleted_at.is_some_and(|value| value < 0)
            || todo.archived_at.is_some_and(|value| value < 0)
            || todo.due_at.is_some_and(|value| value < 0)
            || todo.reminder_at.is_some_and(|value| value < 0)
        {
            return Err("同步文件包含无效时间戳".to_string());
        }
        if todo
            .due_date
            .as_deref()
            .is_some_and(|value| !is_valid_date_only(value))
        {
            return Err("同步文件包含无效到期日期".to_string());
        }
        if todo.due_date.is_some() && todo.due_at.is_some() {
            return Err("同步文件包含重复到期信息".to_string());
        }
        if todo.repeat_rule.is_some() && todo.due_date.is_none() {
            return Err("同步文件包含缺少到期日期的重复任务".to_string());
        }
        validate_repeat_fields(
            todo.repeat_rule.as_deref(),
            todo.repeat_next_due_date.as_deref(),
            "同步文件",
        )?;
    }
    Ok(())
}

fn compare_todos(left: &SyncTodo, right: &SyncTodo) -> Ordering {
    left.updated_at
        .cmp(&right.updated_at)
        .then_with(|| left.deleted_at.is_some().cmp(&right.deleted_at.is_some()))
        .then_with(|| left.archived_at.is_some().cmp(&right.archived_at.is_some()))
        .then_with(|| left.updated_by.cmp(&right.updated_by))
        .then_with(|| left.repeat_series_uuid.cmp(&right.repeat_series_uuid))
        .then_with(|| left.note.cmp(&right.note))
        .then_with(|| canonical_tie_break(left).cmp(&canonical_tie_break(right)))
}

fn compare_groups(left: &SyncGroup, right: &SyncGroup) -> Ordering {
    left.updated_at
        .cmp(&right.updated_at)
        .then_with(|| left.deleted_at.is_some().cmp(&right.deleted_at.is_some()))
        .then_with(|| left.updated_by.cmp(&right.updated_by))
        .then_with(|| {
            (
                left.sort_order,
                left.name.as_str(),
                left.color.as_str(),
                left.created_at,
            )
                .cmp(&(
                    right.sort_order,
                    right.name.as_str(),
                    right.color.as_str(),
                    right.created_at,
                ))
        })
}

fn collapse_duplicate_active_repeat_instances(
    mut todos: Vec<SyncTodo>,
    generated_at: i64,
    updated_by: &str,
) -> Vec<SyncTodo> {
    let mut active_by_repeat = BTreeMap::<(String, String, String), usize>::new();

    for index in 0..todos.len() {
        let Some(key) = active_repeat_instance_key(&todos[index]) else {
            continue;
        };

        let Some(&winner_index) = active_by_repeat.get(&key) else {
            active_by_repeat.insert(key, index);
            continue;
        };

        if compare_todos(&todos[index], &todos[winner_index]).is_gt() {
            mark_repeat_duplicate_deleted(&mut todos[winner_index], generated_at, updated_by);
            active_by_repeat.insert(key, index);
        } else {
            mark_repeat_duplicate_deleted(&mut todos[index], generated_at, updated_by);
        }
    }

    todos
}

fn active_repeat_instance_key(todo: &SyncTodo) -> Option<(String, String, String)> {
    if todo.completed || todo.deleted_at.is_some() || todo.archived_at.is_some() {
        return None;
    }

    Some((
        todo.repeat_series_uuid.clone()?,
        todo.due_date.clone()?,
        todo.repeat_rule.clone()?,
    ))
}

fn mark_repeat_duplicate_deleted(todo: &mut SyncTodo, generated_at: i64, updated_by: &str) {
    let deleted_at = generated_at.max(todo.updated_at);
    todo.deleted_at = Some(deleted_at);
    todo.updated_at = deleted_at;
    todo.updated_by = updated_by.to_string();
}

fn canonical_tie_break(
    todo: &SyncTodo,
) -> (
    bool,
    bool,
    Option<i64>,
    Option<&str>,
    Option<&str>,
    Option<i64>,
    Option<i64>,
    Option<&str>,
    Option<&str>,
    i64,
    &str,
    i64,
) {
    (
        todo.completed,
        todo.pinned,
        todo.completed_at,
        todo.group_uuid.as_deref(),
        todo.due_date.as_deref(),
        todo.due_at,
        todo.reminder_at,
        todo.repeat_rule.as_deref(),
        todo.repeat_next_due_date.as_deref(),
        todo.sort_order,
        todo.title.as_str(),
        todo.created_at,
    )
}

fn map_sync_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncTodo> {
    Ok(SyncTodo {
        uuid: row.get(0)?,
        title: row.get(1)?,
        group_uuid: row.get(2)?,
        completed: row.get::<_, i64>(3)? != 0,
        pinned: row.get::<_, i64>(4)? != 0,
        sort_order: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        completed_at: row.get(8)?,
        deleted_at: row.get(9)?,
        archived_at: row.get(10)?,
        due_date: row.get(11)?,
        due_at: row.get(12)?,
        reminder_at: row.get(13)?,
        repeat_rule: row.get(14)?,
        repeat_next_due_date: row.get(15)?,
        repeat_series_uuid: row.get(16)?,
        note: row.get(17)?,
        updated_by: row.get(18)?,
    })
}

fn canonicalize_repeat_schedule(mut todo: SyncTodo) -> Result<SyncTodo, String> {
    if todo.repeat_rule.is_some() && todo.due_date.is_none() {
        let timestamp = todo
            .due_at
            .ok_or_else(|| "重复任务缺少到期时间".to_string())?;
        todo.due_date = Some(local_date_from_timestamp(timestamp)?);
        todo.due_at = None;
    }
    Ok(todo)
}

fn schedule_for_local_storage(
    todo: &SyncTodo,
    existing_due_at: Option<i64>,
) -> Result<(Option<String>, Option<i64>), String> {
    if todo.repeat_rule.is_none() {
        return Ok((todo.due_date.clone(), todo.due_at));
    }

    let date = todo
        .due_date
        .as_deref()
        .ok_or_else(|| "同步重复任务缺少到期日期".to_string())?;
    Ok((None, Some(timestamp_for_local_date(date, existing_due_at)?)))
}

fn validate_repeat_fields(
    repeat_rule: Option<&str>,
    repeat_next_due_date: Option<&str>,
    source: &str,
) -> Result<(), String> {
    if let Some(rule) = repeat_rule {
        match rule {
            "daily" | "weekly" | "monthly" | "weekdays" => {}
            _ => return Err(format!("{source}包含无效重复规则")),
        }
        let Some(next_due_date) = repeat_next_due_date else {
            return Err(format!("{source}包含缺失的下次重复日期"));
        };
        if !is_valid_date_only(next_due_date) {
            return Err(format!("{source}包含无效下次重复日期"));
        }
    } else if repeat_next_due_date.is_some() {
        return Err(format!("{source}包含孤立的下次重复日期"));
    }
    Ok(())
}

fn map_sync_group(row: &rusqlite::Row<'_>) -> rusqlite::Result<SyncGroup> {
    Ok(SyncGroup {
        uuid: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        deleted_at: row.get(6)?,
        updated_by: row.get(7)?,
    })
}

fn is_valid_date_only(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }

    let Ok(year) = value[0..4].parse::<u32>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u32>() else {
        return false;
    };
    let Ok(day) = value[8..10].parse::<u32>() else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn database_error(error: rusqlite::Error) -> String {
    format!("数据库操作失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{configure_connection, migrate};

    const DEVICE_A: &str = "00000000-0000-4000-8000-00000000000a";
    const DEVICE_B: &str = "00000000-0000-4000-8000-00000000000b";
    const TODO_ID: &str = "00000000-0000-4000-8000-000000000001";
    const NEXT_A_ID: &str = "00000000-0000-4000-8000-000000000101";
    const NEXT_B_ID: &str = "00000000-0000-4000-8000-000000000102";

    fn document(device_id: &str, todos: Vec<SyncTodo>) -> SyncDocument {
        SyncDocument {
            format_version: SYNC_FORMAT_VERSION,
            device_id: device_id.to_string(),
            generated_at: 20,
            groups: vec![],
            todos,
        }
    }

    fn todo(title: &str, updated_at: i64, updated_by: &str) -> SyncTodo {
        SyncTodo {
            uuid: TODO_ID.to_string(),
            title: title.to_string(),
            note: None,
            group_uuid: None,
            completed: false,
            pinned: false,
            sort_order: 0,
            created_at: 1,
            updated_at,
            completed_at: None,
            deleted_at: None,
            archived_at: None,
            due_date: None,
            due_at: None,
            reminder_at: None,
            repeat_rule: None,
            repeat_next_due_date: None,
            repeat_series_uuid: None,
            updated_by: updated_by.to_string(),
        }
    }

    #[test]
    fn builds_a_versioned_document_with_persisted_device_id() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();

        let first = build_document(&connection, 10).unwrap();
        let second = build_document(&connection, 11).unwrap();

        assert_eq!(first.format_version, SYNC_FORMAT_VERSION);
        assert!(first.groups.is_empty());
        assert_eq!(first.device_id, second.device_id);
        assert!(Uuid::parse_str(&first.device_id).is_ok());
    }

    #[test]
    fn stores_synced_repeating_dates_as_local_due_times() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        let repeating = repeating_todo(TODO_ID, "2026-06-10", 10, DEVICE_B);

        merge_remote_document(&mut connection, &document(DEVICE_B, vec![repeating]), 20).unwrap();

        let (due_date, due_at): (Option<String>, Option<i64>) = connection
            .query_row(
                "SELECT due_date, due_at FROM todos WHERE uuid = ?1",
                params![TODO_ID],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(due_date, None);
        assert_eq!(
            due_at,
            Some(timestamp_for_local_date("2026-06-10", None).unwrap())
        );

        let exported = build_document(&connection, 21).unwrap();
        assert_eq!(exported.todos[0].due_date.as_deref(), Some("2026-06-10"));
        assert_eq!(exported.todos[0].due_at, None);
    }

    #[test]
    fn newer_update_wins_independent_of_merge_direction() {
        let older = document(DEVICE_A, vec![todo("older", 10, DEVICE_A)]);
        let newer = document(DEVICE_B, vec![todo("newer", 11, DEVICE_B)]);

        let left = merge_documents(&older, &newer, 30).unwrap();
        let right = merge_documents(&newer, &older, 30).unwrap();

        assert_eq!(left.todos[0].title, "newer");
        assert_eq!(left.todos, right.todos);
    }

    #[test]
    fn deletion_wins_an_equal_timestamp_conflict() {
        let active = document(DEVICE_B, vec![todo("active", 10, DEVICE_B)]);
        let mut deleted_todo = todo("deleted", 10, DEVICE_A);
        deleted_todo.deleted_at = Some(10);
        let deleted = document(DEVICE_A, vec![deleted_todo]);

        let merged = merge_documents(&active, &deleted, 30).unwrap();

        assert_eq!(merged.todos[0].deleted_at, Some(10));
    }

    #[test]
    fn device_id_stably_resolves_equal_time_sort_conflicts() {
        let mut first = todo("todo", 10, DEVICE_A);
        first.sort_order = 1024;
        let mut second = todo("todo", 10, DEVICE_B);
        second.sort_order = 2048;

        let left = merge_documents(
            &document(DEVICE_A, vec![first]),
            &document(DEVICE_B, vec![second]),
            30,
        )
        .unwrap();
        let right = merge_documents(
            &document(DEVICE_B, vec![todo_with_order(2048, DEVICE_B)]),
            &document(DEVICE_A, vec![todo_with_order(1024, DEVICE_A)]),
            30,
        )
        .unwrap();

        assert_eq!(left.todos[0].sort_order, 2048);
        assert_eq!(left.todos, right.todos);
    }

    #[test]
    fn combines_unique_records_from_both_devices() {
        let local = document(DEVICE_A, vec![todo("local", 10, DEVICE_A)]);
        let mut remote_todo = todo("remote", 10, DEVICE_B);
        remote_todo.uuid = "00000000-0000-4000-8000-000000000002".to_string();
        let remote = document(DEVICE_B, vec![remote_todo]);

        let merged = merge_documents(&local, &remote, 30).unwrap();

        assert_eq!(merged.todos.len(), 2);
        assert_eq!(merged.device_id, DEVICE_A);
    }

    #[test]
    fn deduplicates_next_repeat_instance_from_simultaneous_completion() {
        let series_uuid = TODO_ID.to_string();
        let mut local_completed = repeating_todo(TODO_ID, "2026-06-10", 100, DEVICE_A);
        local_completed.completed = true;
        local_completed.completed_at = Some(100);
        let local_next = repeating_todo(NEXT_A_ID, "2026-06-11", 100, DEVICE_A);

        let mut remote_completed = repeating_todo(TODO_ID, "2026-06-10", 100, DEVICE_B);
        remote_completed.completed = true;
        remote_completed.completed_at = Some(100);
        let remote_next = repeating_todo(NEXT_B_ID, "2026-06-11", 100, DEVICE_B);

        let merged = merge_documents(
            &document(DEVICE_A, vec![local_completed, local_next]),
            &document(DEVICE_B, vec![remote_completed, remote_next]),
            120,
        )
        .unwrap();

        let next_instances = merged
            .todos
            .iter()
            .filter(|todo| {
                todo.repeat_series_uuid.as_deref() == Some(series_uuid.as_str())
                    && todo.due_date.as_deref() == Some("2026-06-11")
            })
            .collect::<Vec<_>>();
        let visible_next_instances = next_instances
            .iter()
            .filter(|todo| !todo.completed && todo.deleted_at.is_none())
            .count();
        let deleted_duplicates = next_instances
            .iter()
            .filter(|todo| todo.deleted_at == Some(120))
            .count();

        assert_eq!(next_instances.len(), 2);
        assert_eq!(visible_next_instances, 1);
        assert_eq!(deleted_duplicates, 1);
    }

    #[test]
    fn persists_remote_updates_and_soft_deletes_by_uuid() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();
        let local_device = device_id(&connection).unwrap();
        let mut local_todo = todo("local", 10, &local_device);
        local_todo.uuid = TODO_ID.to_string();
        merge_remote_document(
            &mut connection,
            &document(&local_device, vec![local_todo]),
            20,
        )
        .unwrap();

        let mut remote_todo = todo("remote deleted", 11, DEVICE_B);
        remote_todo.pinned = true;
        remote_todo.note = Some("remote note".to_string());
        remote_todo.due_date = Some("2026-06-10".to_string());
        remote_todo.archived_at = Some(11);
        remote_todo.deleted_at = Some(11);
        let merged =
            merge_remote_document(&mut connection, &document(DEVICE_B, vec![remote_todo]), 30)
                .unwrap();

        assert_eq!(merged.todos[0].title, "remote deleted");
        let persisted = build_document(&connection, 31).unwrap();
        assert_eq!(persisted.todos[0].deleted_at, Some(11));
        assert!(persisted.todos[0].pinned);
        assert_eq!(persisted.todos[0].note.as_deref(), Some("remote note"));
        assert_eq!(persisted.todos[0].archived_at, Some(11));
        assert_eq!(persisted.todos[0].due_date.as_deref(), Some("2026-06-10"));
        assert_eq!(persisted.todos[0].updated_by, DEVICE_B);
    }

    #[test]
    fn persists_remote_groups_and_todo_membership() {
        let mut connection = Connection::open_in_memory().unwrap();
        configure_connection(&connection).unwrap();
        migrate(&mut connection).unwrap();

        let group = SyncGroup {
            uuid: "00000000-0000-4000-8000-0000000000aa".to_string(),
            name: "工作".to_string(),
            color: "yellow".to_string(),
            sort_order: 0,
            created_at: 1,
            updated_at: 2,
            deleted_at: None,
            updated_by: DEVICE_B.to_string(),
        };
        let mut grouped_todo = todo("remote grouped", 3, DEVICE_B);
        grouped_todo.group_uuid = Some(group.uuid.clone());

        merge_remote_document(
            &mut connection,
            &SyncDocument {
                format_version: SYNC_FORMAT_VERSION,
                device_id: DEVICE_B.to_string(),
                generated_at: 4,
                groups: vec![group.clone()],
                todos: vec![grouped_todo],
            },
            5,
        )
        .unwrap();

        let persisted = build_document(&connection, 6).unwrap();
        assert_eq!(persisted.groups, vec![group]);
        assert_eq!(
            persisted.todos[0].group_uuid.as_deref(),
            Some(persisted.groups[0].uuid.as_str())
        );
    }

    #[test]
    fn reads_legacy_sync_document_without_pinned_field() {
        let json = format!(
            r#"{{
                "format_version": 1,
                "device_id": "{DEVICE_A}",
                "generated_at": 20,
                "todos": [{{
                    "uuid": "{TODO_ID}",
                    "title": "legacy",
                    "completed": false,
                    "sort_order": 0,
                    "created_at": 1,
                    "updated_at": 1,
                    "completed_at": null,
                    "deleted_at": null,
                    "updated_by": "{DEVICE_A}"
                }}]
            }}"#
        );

        let document: SyncDocument = serde_json::from_str(&json).unwrap();
        assert!(!document.todos[0].pinned);
        assert_eq!(document.todos[0].note, None);
        assert!(document.groups.is_empty());
        assert_eq!(document.todos[0].archived_at, None);
        assert_eq!(document.todos[0].due_date, None);
        validate_document(&document).unwrap();
    }

    fn todo_with_order(sort_order: i64, updated_by: &str) -> SyncTodo {
        let mut value = todo("todo", 10, updated_by);
        value.sort_order = sort_order;
        value
    }

    fn repeating_todo(uuid: &str, due_date: &str, updated_at: i64, updated_by: &str) -> SyncTodo {
        let mut value = todo("repeat", updated_at, updated_by);
        value.uuid = uuid.to_string();
        value.due_date = Some(due_date.to_string());
        value.repeat_rule = Some("daily".to_string());
        value.repeat_next_due_date = Some(next_day(due_date));
        value.repeat_series_uuid = Some(TODO_ID.to_string());
        value
    }

    fn next_day(date: &str) -> String {
        match date {
            "2026-06-10" => "2026-06-11".to_string(),
            "2026-06-11" => "2026-06-12".to_string(),
            _ => panic!("unexpected test date"),
        }
    }
}
