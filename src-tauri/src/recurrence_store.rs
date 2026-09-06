//! Rule persistence only. The task-completion orchestrator is intentionally not wired yet.
use rusqlite::{params, Connection};

use crate::recurrence_protocol::{
    encode_document, merge_documents, parse_document, RecurrenceDocument, RecurrenceRule,
};

#[derive(Debug)]
pub struct RuleSnapshot {
    pub revision: i64,
    pub synced_revision: i64,
    pub etag: Option<String>,
    pub document: RecurrenceDocument,
}

fn db_error(error: rusqlite::Error) -> String {
    format!("RECURRENCE_DATABASE: {error}")
}

pub fn snapshot(connection: &Connection) -> Result<RuleSnapshot, String> {
    // Read revision first: any intervening write makes acknowledgement fail conservatively.
    let (revision, synced_revision, etag) = connection
        .query_row(
            "SELECT revision, synced_revision, etag FROM recurrence_sync_state WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(db_error)?;
    let mut statement = connection
        .prepare("SELECT record_json FROM recurrence_rules ORDER BY uuid")
        .map_err(db_error)?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(db_error)?;
    let mut rules: Vec<RecurrenceRule> = Vec::new();
    for row in rows {
        let document = parse_document(&format!(
            "{{\"format_version\":1,\"rules\":[{}]}}",
            row.map_err(db_error)?
        ))?;
        rules.extend(document.rules);
    }
    let document = RecurrenceDocument {
        format_version: 1,
        rules,
    };
    encode_document(&document)?;
    Ok(RuleSnapshot {
        revision,
        synced_revision,
        etag,
        document,
    })
}

pub fn merge(
    connection: &mut Connection,
    incoming: &RecurrenceDocument,
) -> Result<RuleSnapshot, String> {
    let transaction = connection.transaction().map_err(db_error)?;
    let result = merge_in_transaction(&transaction, incoming)?;
    transaction.commit().map_err(db_error)?;
    Ok(result)
}

pub(crate) fn merge_in_transaction(
    transaction: &Connection,
    incoming: &RecurrenceDocument,
) -> Result<RuleSnapshot, String> {
    encode_document(incoming)?;
    let current = snapshot(&transaction)?;
    let mut merged = merge_documents(&current.document, incoming)?;
    encode_document(&merged)?;
    // Release old active links before binding a replacement series in the same transaction.
    merged
        .rules
        .sort_by_key(|rule| rule.deleted_at.is_none() && !rule.exhausted);
    for rule in merged.rules {
        let record = serde_json::to_string(&rule).map_err(|e| e.to_string())?;
        transaction.execute(
            "INSERT INTO recurrence_rules(uuid,current_todo_uuid,active,record_json) VALUES (?1,?2,?3,?4)
             ON CONFLICT(uuid) DO UPDATE SET current_todo_uuid=excluded.current_todo_uuid,
             active=excluded.active,record_json=excluded.record_json WHERE record_json<>excluded.record_json",
            params![rule.uuid, rule.current_todo_uuid, rule.deleted_at.is_none() && !rule.exhausted, record]).map_err(db_error)?;
    }
    let result = snapshot(&transaction)?;
    Ok(result)
}

pub fn acknowledge(connection: &Connection, revision: i64, etag: &str) -> Result<bool, String> {
    if !(0..=9_007_199_254_740_991).contains(&revision)
        || etag.is_empty()
        || etag.len() > 1024
        || !etag.bytes().all(|c| (32..=126).contains(&c))
    {
        return Err("INVALID_RECURRENCE_ACK".to_string());
    }
    connection.execute("UPDATE recurrence_sync_state SET synced_revision=?1,etag=?2 WHERE id=1 AND revision=?1",
        params![revision, etag]).map(|count| count == 1).map_err(db_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Connection, RecurrenceDocument) {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut connection).unwrap();
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../docs/fixtures/recurrence-document-v1.json"
        ))
        .unwrap();
        let document = parse_document(&fixtures["valid"][1]["document"].to_string()).unwrap();
        (connection, document)
    }

    #[test]
    fn persists_idempotently_and_keeps_edits_dirty_after_stale_ack() {
        let (mut connection, mut document) = setup();
        let saved = merge(&mut connection, &document).unwrap();
        assert_eq!(saved.revision, 1);
        assert!(acknowledge(&connection, 1, "\"first\"").unwrap());
        assert_eq!(merge(&mut connection, &document).unwrap().revision, 1);
        document.rules[0].updated_at += 1;
        let next = merge(&mut connection, &document).unwrap();
        assert_eq!(next.revision, 2);
        assert!(!acknowledge(&connection, 1, "\"stale\"").unwrap());
        assert_eq!(snapshot(&connection).unwrap().synced_revision, 1);
        document.rules[0].deleted_at = Some(1000);
        let stopped = merge(&mut connection, &document).unwrap();
        document.rules[0].deleted_at = None;
        document.rules[0].updated_at += 100;
        assert_eq!(
            merge(&mut connection, &document).unwrap().document,
            stopped.document
        );
    }

    #[test]
    fn failed_multirow_merge_rolls_back_rules_and_revision() {
        let (mut connection, mut document) = setup();
        let before = merge(&mut connection, &document).unwrap();
        let mut second = document.rules[0].clone();
        second.uuid = "123e4567-e89b-42d3-a456-426614174002".to_string();
        second.first_todo_uuid = "123e4567-e89b-42d3-a456-426614174003".to_string();
        second.current_todo_uuid = second.first_todo_uuid.clone();
        document.rules[0].deleted_at = Some(1000);
        document.rules.push(second);
        connection
            .execute_batch(
                "CREATE TRIGGER fail_new_rule BEFORE INSERT ON recurrence_rules
          WHEN NEW.uuid='123e4567-e89b-42d3-a456-426614174002'
          BEGIN SELECT RAISE(ABORT,'injected failure'); END;",
            )
            .unwrap();
        assert!(merge(&mut connection, &document).is_err());
        let after = snapshot(&connection).unwrap();
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.document, before.document);
    }

    #[test]
    fn v16_upgrade_is_additive_and_idempotent() {
        let (mut connection, document) = setup();
        connection
            .execute_batch(
                "DROP TABLE recurrence_rules; DROP TABLE recurrence_sync_state;
          DELETE FROM schema_migrations WHERE version=17;
          INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by)
          VALUES ('upgrade-task','keep task',0,1,1,'device');",
            )
            .unwrap();
        crate::db::migrate(&mut connection).unwrap();
        merge(&mut connection, &document).unwrap();
        crate::db::migrate(&mut connection).unwrap();
        assert_eq!(snapshot(&connection).unwrap().revision, 1);
        assert_eq!(
            connection
                .query_row(
                    "SELECT title FROM todos WHERE uuid='upgrade-task'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "keep task"
        );
    }

    #[test]
    fn legacy_todo_document_roundtrip_does_not_touch_rules() {
        let (mut connection, document) = setup();
        connection
            .execute(
                "INSERT INTO todos(uuid,title,sort_order,created_at,updated_at,updated_by)
          VALUES (?1,'plain task',0,1,1,?2)",
                params![
                    document.rules[0].first_todo_uuid,
                    crate::db::device_id(&connection).unwrap()
                ],
            )
            .unwrap();
        let before = merge(&mut connection, &document).unwrap();
        let old_document = crate::sync::build_document(&connection, 2000).unwrap();
        let raw = serde_json::to_string(&old_document).unwrap();
        assert!(!raw.contains("anchor_date"));
        assert_eq!(old_document.todos[0].repeat_rule, None);
        crate::sync::merge_remote_document(&mut connection, &old_document, 3000).unwrap();
        let after = snapshot(&connection).unwrap();
        assert_eq!(after.document, before.document);
        assert_eq!(after.revision, before.revision);
    }
}
