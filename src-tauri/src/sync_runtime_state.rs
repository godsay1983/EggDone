use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::db::now_millis;

const STATE_ID: i64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncDomain {
    Todos,
    Notes,
    Attachments,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncAttemptRevisions {
    todos: i64,
    notes: i64,
    attachments: i64,
}

#[cfg(test)]
impl SyncAttemptRevisions {
    pub fn for_domain(self, domain: SyncDomain) -> i64 {
        match domain {
            SyncDomain::Todos => self.todos,
            SyncDomain::Notes => self.notes,
            SyncDomain::Attachments => self.attachments,
        }
    }
}

impl SyncDomain {
    fn as_str(self) -> &'static str {
        match self {
            Self::Todos => "todos",
            Self::Notes => "notes",
            Self::Attachments => "attachments",
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SyncRuntimeSnapshot {
    pub schema_version: i64,
    pub last_attempt_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub dirty_since: Option<i64>,
    pub dirty_domains: Vec<String>,
    pub last_result: String,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub pending_attachment_count: i64,
    pub updated_at: i64,
}

pub fn get_snapshot(connection: &Connection) -> Result<SyncRuntimeSnapshot, String> {
    refresh_pending_attachment_count(connection)?;
    connection
        .query_row(
            "SELECT schema_version, last_attempt_at, last_success_at, dirty_since,
                    dirty_domains, last_result, last_error_code, last_error_message,
                    pending_attachment_count, updated_at,
                    (SELECT revision>synced_revision FROM task_note_link_sync_state WHERE id=1)
             FROM sync_runtime_state WHERE id = ?1",
            params![STATE_ID],
            |row| {
                let dirty_json: String = row.get(4)?;
                let mut domains: Vec<String> =
                    serde_json::from_str(&dirty_json).unwrap_or_default();
                if row.get::<_, bool>(10)? {
                    domains.push("links".into());
                }
                Ok(SyncRuntimeSnapshot {
                    schema_version: row.get(0)?,
                    last_attempt_at: row.get(1)?,
                    last_success_at: row.get(2)?,
                    dirty_since: row.get(3)?,
                    dirty_domains: domains,
                    last_result: row.get(5)?,
                    last_error_code: row.get(6)?,
                    last_error_message: row.get(7)?,
                    pending_attachment_count: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .map_err(database_error)
}

pub fn begin_attempt(connection: &Connection) -> Result<SyncAttemptRevisions, String> {
    let now = now_millis();
    connection
        .execute(
            "UPDATE sync_runtime_state
             SET last_attempt_at = ?1, last_result = 'interrupted',
                 last_error_code = NULL, last_error_message = NULL, updated_at = ?1
             WHERE id = ?2",
            params![now, STATE_ID],
        )
        .map_err(database_error)?;
    connection
        .query_row(
            "SELECT todos_dirty_version, notes_dirty_version, attachments_dirty_version
             FROM sync_runtime_state WHERE id = ?1",
            params![STATE_ID],
            |row| {
                Ok(SyncAttemptRevisions {
                    todos: row.get(0)?,
                    notes: row.get(1)?,
                    attachments: row.get(2)?,
                })
            },
        )
        .map_err(database_error)
}

pub fn mark_domain_synced(
    connection: &Connection,
    domain: SyncDomain,
    expected_revision: i64,
) -> Result<bool, String> {
    if domain == SyncDomain::Attachments {
        let pending:i64=connection.query_row("SELECT COUNT(*) FROM note_attachments WHERE deleted_at IS NULL AND remote_uploaded=0",[],|r|r.get(0)).map_err(database_error)?;
        if pending > 0 {
            return Ok(false);
        }
    }
    let version_column = match domain {
        SyncDomain::Todos => "todos_dirty_version",
        SyncDomain::Notes => "notes_dirty_version",
        SyncDomain::Attachments => "attachments_dirty_version",
    };
    let current_revision: i64 = connection
        .query_row(
            &format!("SELECT {version_column} FROM sync_runtime_state WHERE id = ?1"),
            params![STATE_ID],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if current_revision != expected_revision {
        return Ok(false);
    }
    let dirty_json: String = connection
        .query_row(
            "SELECT dirty_domains FROM sync_runtime_state WHERE id = ?1",
            params![STATE_ID],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    let mut domains: Vec<String> = serde_json::from_str(&dirty_json).unwrap_or_default();
    domains.retain(|value| value != domain.as_str());
    let dirty_since = if domains.is_empty() {
        None
    } else {
        connection
            .query_row(
                "SELECT dirty_since FROM sync_runtime_state WHERE id = ?1",
                params![STATE_ID],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
            .map_err(database_error)?
            .flatten()
    };
    connection
        .execute(
            "UPDATE sync_runtime_state
             SET dirty_domains = ?1, dirty_since = ?2, updated_at = ?3
             WHERE id = ?4 AND dirty_domains = ?5
             AND CASE ?6 WHEN 'todos' THEN todos_dirty_version WHEN 'notes' THEN notes_dirty_version
                 ELSE attachments_dirty_version END = ?7",
            params![
                serde_json::to_string(&domains).map_err(|error| error.to_string())?,
                dirty_since,
                now_millis(),
                STATE_ID,
                dirty_json,
                domain.as_str(),
                expected_revision
            ],
        )
        .map(|changed| changed == 1)
        .map_err(database_error)
}

pub(crate) fn domain_revision(connection: &Connection, domain: SyncDomain) -> Result<i64, String> {
    connection.query_row("SELECT CASE ?1 WHEN 'todos' THEN todos_dirty_version WHEN 'notes' THEN notes_dirty_version ELSE attachments_dirty_version END FROM sync_runtime_state WHERE id=1",
        [domain.as_str()],|row|row.get(0)).map_err(database_error)
}

// Rule-only concurrent edits still need to be visible to the existing aggregate Todo status.
pub(crate) fn retain_todos_pending(connection: &Connection) -> Result<(), String> {
    let tx = connection.unchecked_transaction().map_err(database_error)?;
    retain_todos_pending_in_transaction(&tx)?;
    tx.commit().map_err(database_error)
}

pub(crate) fn retain_todos_pending_in_transaction(tx: &Connection) -> Result<(), String> {
    let raw: String = tx
        .query_row(
            "SELECT dirty_domains FROM sync_runtime_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(database_error)?;
    let mut domains: Vec<String> = serde_json::from_str(&raw).map_err(|_| "SYNC_STATE_INVALID")?;
    if !domains.iter().any(|d| d == "todos") {
        domains.push("todos".into());
    }
    tx.execute("UPDATE sync_runtime_state SET dirty_domains=?1,dirty_since=COALESCE(dirty_since,?2) WHERE id=1",
        params![serde_json::to_string(&domains).map_err(|_|"SYNC_STATE_INVALID")?,now_millis()]).map_err(database_error)?;
    Ok(())
}

pub fn record_success(connection: &Connection) -> Result<(), String> {
    let now = now_millis();
    refresh_pending_attachment_count(connection)?;
    connection
        .execute(
            "UPDATE sync_runtime_state
             SET last_success_at = ?1, last_result = 'success',
                 last_error_code = NULL, last_error_message = NULL, updated_at = ?1
             WHERE id = ?2",
            params![now, STATE_ID],
        )
        .map(|_| ())
        .map_err(database_error)
}

pub fn record_failure(connection: &Connection, error: &str) -> Result<(), String> {
    let (result, code, message) = classify_error(error);
    let now = now_millis();
    refresh_pending_attachment_count(connection)?;
    connection
        .execute(
            "UPDATE sync_runtime_state
             SET last_result = ?1, last_error_code = ?2,
                 last_error_message = ?3, updated_at = ?4
             WHERE id = ?5",
            params![result, code, message, now, STATE_ID],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn refresh_pending_attachment_count(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "UPDATE sync_runtime_state
             SET pending_attachment_count = (
                 SELECT COUNT(*) FROM note_attachments
                 WHERE deleted_at IS NULL
                   AND transfer_state IN ('pending_upload', 'uploading', 'uploaded', 'failed')
                   AND NOT (transfer_state = 'failed' AND remote_uploaded = 1)
             )
             WHERE id = ?1",
            params![STATE_ID],
        )
        .map(|_| ())
        .map_err(database_error)
}

fn classify_error(error: &str) -> (&'static str, &'static str, &'static str) {
    let lower = error.to_lowercase();
    if lower.contains("远端文件持续发生变化") || lower.contains("conflict") {
        return ("conflict", "SYNC_CONFLICT", "远端数据在同步期间持续变化");
    }
    if [
        "凭据",
        "密钥",
        "access key",
        "secret key",
        "配置",
        "recurrence_config_changed",
        "sync_target_save_incomplete",
        "_http:403",
        "_http:401",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
    {
        return ("failed", "SYNC_CREDENTIALS", "同步凭据或配置无效");
    }
    if [
        "network",
        "_http:408",
        "_http:429",
        "_http:500",
        "_http:502",
        "_http:503",
        "_http:504",
        "连接",
        "网络",
        "超时",
        "timeout",
        "offline",
        "connection",
        "dns",
        "status code 408",
        "status code 425",
        "status code 429",
        "状态码 408",
        "状态码 425",
        "状态码 429",
        "状态码 500",
        "状态码 502",
        "状态码 503",
        "状态码 504",
    ]
    .iter()
    .any(|keyword| lower.contains(keyword))
    {
        return ("offline", "SYNC_OFFLINE", "网络暂时不可用");
    }
    if lower.contains("附件") || lower.contains("asset") {
        return ("failed", "SYNC_ATTACHMENT", "附件同步未完成");
    }
    ("failed", "SYNC_FAILED", "同步未完成")
}

fn database_error(error: rusqlite::Error) -> String {
    format!("同步状态数据库操作失败：{error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_binary_cannot_be_confirmed_by_a_metadata_snapshot() {
        let db = Connection::open_in_memory().unwrap();
        // Minimal real SQLite tables isolate the ACK contract from attachment import validation.
        db.execute_batch("CREATE TABLE sync_runtime_state(id INTEGER PRIMARY KEY,dirty_domains TEXT,dirty_since INTEGER,
            updated_at INTEGER,todos_dirty_version INTEGER,notes_dirty_version INTEGER,attachments_dirty_version INTEGER);
            INSERT INTO sync_runtime_state VALUES(1,'[\"attachments\"]',1,1,0,0,2);
            CREATE TABLE note_attachments(deleted_at INTEGER,remote_uploaded INTEGER);
            INSERT INTO note_attachments VALUES(NULL,0);").unwrap();
        assert!(!mark_domain_synced(&db, SyncDomain::Attachments, 2).unwrap());
        db.execute("UPDATE note_attachments SET remote_uploaded=1", [])
            .unwrap();
        assert!(mark_domain_synced(&db, SyncDomain::Attachments, 2).unwrap());
        retain_todos_pending(&db).unwrap();
        retain_todos_pending(&db).unwrap();
        assert_eq!(
            db.query_row("SELECT dirty_domains FROM sync_runtime_state", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "[\"todos\"]"
        );
    }

    #[test]
    fn diagnostics_do_not_persist_raw_secrets() {
        let (_, code, message) = classify_error(
            "Access Key AKIA_TEST and Secret Key private-value caused connection failure",
        );
        assert_eq!(code, "SYNC_CREDENTIALS");
        assert_eq!(message, "同步凭据或配置无效");
        assert!(!message.contains("AKIA_TEST"));
        assert!(!message.contains("private-value"));
    }

    #[test]
    fn a_write_during_sync_keeps_the_domain_dirty() {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::db::configure_connection(&connection).unwrap();
        crate::db::migrate(&mut connection).unwrap();
        let identity = crate::db::device_id(&connection).unwrap();
        connection
            .execute(
                "INSERT INTO todos (
                    uuid, title, completed, sort_order, created_at, updated_at, updated_by
                 ) VALUES (?1, 'before upload', 0, 0, 1, 1, ?2)",
                params!["00000000-0000-4000-8000-000000000018", identity],
            )
            .unwrap();
        let attempt = begin_attempt(&connection).unwrap();
        connection
            .execute(
                "UPDATE todos SET title = 'during upload', updated_at = 2 WHERE uuid = ?1",
                params!["00000000-0000-4000-8000-000000000018"],
            )
            .unwrap();

        assert!(!mark_domain_synced(
            &connection,
            SyncDomain::Todos,
            attempt.for_domain(SyncDomain::Todos),
        )
        .unwrap());
        assert_eq!(
            get_snapshot(&connection).unwrap().dirty_domains,
            vec!["todos"]
        );

        let next_attempt = begin_attempt(&connection).unwrap();
        assert!(mark_domain_synced(
            &connection,
            SyncDomain::Todos,
            next_attempt.for_domain(SyncDomain::Todos),
        )
        .unwrap());
        assert!(get_snapshot(&connection).unwrap().dirty_domains.is_empty());
    }

    #[test]
    fn a_partial_failure_keeps_only_the_unsynced_domain_dirty() {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::db::configure_connection(&connection).unwrap();
        crate::db::migrate(&mut connection).unwrap();
        connection
            .execute(
                "UPDATE sync_runtime_state
                 SET dirty_domains = '[\"todos\",\"attachments\"]', dirty_since = 1,
                     todos_dirty_version = 1, attachments_dirty_version = 1
                 WHERE id = 1",
                [],
            )
            .unwrap();

        let attempt = begin_attempt(&connection).unwrap();
        assert!(mark_domain_synced(
            &connection,
            SyncDomain::Todos,
            attempt.for_domain(SyncDomain::Todos),
        )
        .unwrap());
        record_failure(&connection, "附件二进制同步失败").unwrap();

        let snapshot = get_snapshot(&connection).unwrap();
        assert_eq!(snapshot.last_result, "failed");
        assert_eq!(snapshot.last_error_code.as_deref(), Some("SYNC_ATTACHMENT"));
        assert_eq!(snapshot.dirty_domains, vec!["attachments"]);
    }
}
