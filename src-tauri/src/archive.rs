//! Archive persistence. UI and system side effects are connected in LC1b.
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const MAX_CLOCK: i64 = 9_007_199_254_740_991;
const TASK_COLUMNS: &str = "uuid,title,COALESCE(note,''),group_uuid,completed,pinned,priority,sort_order,created_at,updated_at,completed_at,deleted_at,archived_at,due_date,due_at,reminder_at,repeat_rule,repeat_next_due_date,repeat_series_uuid,updated_by";
pub(crate) type Rows = Vec<Vec<Option<String>>>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Expected {
    pub uuid: String,
    pub scope: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub expected: Expected,
    pub title: String,
    pub content: String,
    pub completed: bool,
    pub archived_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub due_date: Option<String>,
    pub due_at: Option<i64>,
    pub group_name: Option<String>,
    pub checklist_json: String,
    pub links_json: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Cursor {
    pub archived_at: i64,
    pub uuid: String,
    pub query: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Page {
    pub items: Vec<Preview>,
    pub next: Option<Cursor>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Unarchive,
    Reopen,
    Delete,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Outcome {
    pub uuid: String,
    pub action: Action,
    pub outcome: String,
    pub result_version: i64,
    pub warnings: Vec<String>,
}
#[derive(Deserialize, Serialize)]
struct Receipt {
    request: String,
    result: Outcome,
}
struct Detail {
    preview: Preview,
    group_valid: bool,
    rule_active: bool,
}

pub(crate) fn db(_: rusqlite::Error) -> String {
    "ARCHIVE_DATABASE_FAILED".into()
}
pub(crate) fn json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|_| "ARCHIVE_INVALID_STATE".into())
}
pub(crate) fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub(crate) fn id(value: &str) -> Result<(), String> {
    if Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value) {
        Ok(())
    } else {
        Err("ARCHIVE_INVALID_ID".into())
    }
}
pub(crate) fn version(now: i64, by: &str) -> Result<(), String> {
    if !(0..=MAX_CLOCK).contains(&now) || id(by).is_err() {
        Err("ARCHIVE_INVALID_VERSION".into())
    } else {
        Ok(())
    }
}
pub(crate) fn meta(c: &Connection, key: &str) -> Result<Option<String>, String> {
    c.query_row("SELECT value FROM app_metadata WHERE key=?1", [key], |r| {
        r.get(0)
    })
    .optional()
    .map_err(db)
}
pub(crate) fn put(c: &Connection, key: &str, value: &str) -> Result<(), String> {
    c.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value]).map_err(db)?;
    Ok(())
}
pub(crate) fn scope(c: &Connection) -> Result<String, String> {
    let key = "archive.scope.v1";
    let local = match meta(c, key)? {
        Some(v) => v,
        None => {
            let v = Uuid::new_v4().to_string();
            put(c, key, &v)?;
            v
        }
    };
    let target = meta(c, "sync.target.epoch.v1")?.unwrap_or_else(|| "local".into());
    if target.starts_with("pending:") {
        return Err("ARCHIVE_SCOPE_CHANGED".into());
    }
    Ok(hash(&json(&vec![local, target])?))
}
/// Called inside the import transaction, including legacy imports without template data.
pub fn invalidate_in_transaction(c: &Connection) -> Result<(), String> {
    if c.is_autocommit() {
        return Err("ARCHIVE_TRANSACTION_REQUIRED".into());
    }
    put(c, "archive.scope.v1", &Uuid::new_v4().to_string())?;
    c.execute("DELETE FROM app_metadata WHERE key LIKE 'archive.op.v1:%' OR key LIKE 'archive.batch.v1:%' OR key LIKE 'archive.batch.dismissed.v1:%'",[]).map_err(db)?;
    Ok(())
}
pub(crate) fn rows(
    c: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
) -> Result<Rows, String> {
    let mut q = c.prepare(sql).map_err(db)?;
    let count = q.column_count();
    let result = q
        .query_map(values, |r| {
            (0..count)
                .map(|i| match r.get_ref(i)? {
                    ValueRef::Null => Ok(None),
                    ValueRef::Integer(n) => Ok(Some(n.to_string())),
                    ValueRef::Text(s) => String::from_utf8(s.to_vec())
                        .map(Some)
                        .map_err(|_| rusqlite::Error::InvalidQuery),
                    _ => Err(rusqlite::Error::InvalidQuery),
                })
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(db)?
        .collect::<rusqlite::Result<Rows>>()
        .map_err(db)?;
    Ok(result)
}
fn number(value: &Option<String>) -> Result<i64, String> {
    value
        .as_ref()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| (0..=MAX_CLOCK).contains(v))
        .ok_or_else(|| "ARCHIVE_INVALID_VERSION".into())
}
fn nullable_number(value: &Option<String>) -> Result<Option<i64>, String> {
    if value.is_none() {
        Ok(None)
    } else {
        number(value).map(Some)
    }
}
fn document(c: &Connection, sql: &str, uuid: &str, domain: &str) -> Result<String, String> {
    let values = rows(c, sql, &[&uuid])?;
    let records: Vec<String> = values
        .into_iter()
        .map(|r| r[0].clone().unwrap_or_default())
        .collect();
    let raw = format!(
        "{{\"format_version\":1,\"{domain}\":[{}]}}",
        records.join(",")
    );
    match domain {
        "items" => crate::task_checklist_protocol::encode_items(
            &crate::task_checklist_protocol::parse_items(&raw)?,
        ),
        "links" => crate::task_note_link_protocol::encode_document(
            &crate::task_note_link_protocol::parse_document(&raw)?,
        ),
        _ => Err("ARCHIVE_INVALID_STATE".into()),
    }
}
fn read(c: &Connection, uuid: &str) -> Result<Detail, String> {
    id(uuid)?;
    let task = rows(
        c,
        &format!("SELECT {TASK_COLUMNS} FROM todos WHERE uuid=?1"),
        &[&uuid],
    )?;
    let t = task.first().ok_or("ARCHIVE_NOT_FOUND")?;
    if t[11].is_some() || t[12].is_none() {
        return Err("ARCHIVE_NOT_ARCHIVED".into());
    }
    let group=rows(c,"SELECT uuid,name,color,sort_order,created_at,updated_at,deleted_at,updated_by FROM groups WHERE uuid=?1",&[&t[3]])?;
    let receipt = meta(c, &format!("recurrence.instance.v1:{uuid}"))?;
    let rule_id = match &receipt {
        Some(v) => serde_json::from_str::<serde_json::Value>(v)
            .map_err(|_| "ARCHIVE_INVALID_STATE")?
            .get("rule_uuid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_owned(),
        None => String::new(),
    };
    let rules=rows(c,"SELECT uuid,current_todo_uuid,active,record_json FROM recurrence_rules WHERE current_todo_uuid=?1 OR uuid=?2 ORDER BY uuid",&[&uuid,&rule_id])?;
    let mut rule_records = Vec::new();
    for row in &rules {
        let raw = format!(
            "{{\"format_version\":1,\"rules\":[{}]}}",
            row[3].as_deref().unwrap_or("")
        );
        rule_records.push(crate::recurrence_protocol::encode_document(
            &crate::recurrence_protocol::parse_document(&raw)?,
        )?);
    }
    let rule_active = rules
        .iter()
        .any(|r| r[1].as_deref() == Some(uuid) && r[2].as_deref() == Some("1"));
    let checklist = document(
        c,
        "SELECT record_json FROM task_checklist_items WHERE todo_uuid=?1 ORDER BY uuid",
        uuid,
        "items",
    )?;
    let links = document(
        c,
        "SELECT record_json FROM task_note_links WHERE todo_uuid=?1 ORDER BY uuid",
        uuid,
        "links",
    )?;
    let scope = scope(c)?;
    let rule_headers: Rows = rules.iter().map(|r| r[..3].to_vec()).collect();
    let evidence = json(&(
        task.clone(),
        group.clone(),
        rule_records,
        receipt,
        checklist.clone(),
        links.clone(),
        rule_headers,
    ))?;
    // Legacy built-in repeats store their all-day date as a local timestamp.
    let mut due_date = t[13].clone();
    let mut due_at = nullable_number(&t[14])?;
    if t[16].is_some() && due_date.is_none() {
        if let Some(at) = due_at {
            due_date = Some(
                crate::schedule::local_date_from_timestamp(at)
                    .map_err(|_| "ARCHIVE_INVALID_STATE")?,
            );
        }
    }
    if due_date.is_some() {
        due_at = None;
    }
    Ok(Detail {
        group_valid: group.first().is_some_and(|g| g[6].is_none()),
        rule_active,
        preview: Preview {
            expected: Expected {
                uuid: uuid.into(),
                scope,
                fingerprint: hash(&evidence),
            },
            title: t[1].clone().unwrap_or_default(),
            content: t[2].clone().unwrap_or_default(),
            completed: t[4].as_deref() == Some("1"),
            archived_at: number(&t[12])?,
            updated_at: number(&t[9])?,
            completed_at: nullable_number(&t[10])?,
            due_date,
            due_at,
            group_name: group.first().and_then(|g| g[1].clone()),
            checklist_json: checklist,
            links_json: links,
        },
    })
}
pub fn preview(c: &mut Connection, uuid: &str) -> Result<Preview, String> {
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    let result = read(&tx, uuid)?.preview;
    tx.commit().map_err(db)?;
    Ok(result)
}
pub fn select_all(c: &mut Connection, query: &str) -> Result<Vec<Preview>, String> {
    query_valid(query, 50, None)?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    let keys=rows(&tx,"SELECT uuid FROM todos WHERE archived_at IS NOT NULL AND deleted_at IS NULL AND (instr(title,?1)>0 OR instr(COALESCE(note,''),?1)>0) ORDER BY archived_at DESC,uuid ASC LIMIT 10001",&[&query])?;
    if keys.len() > 10_000 {
        return Err("ARCHIVE_SELECTION_LIMIT".into());
    }
    let mut result = Vec::new();
    for row in keys {
        result.push(read(&tx, row[0].as_deref().ok_or("ARCHIVE_INVALID_STATE")?)?.preview);
    }
    tx.commit().map_err(db)?;
    Ok(result)
}
fn query_valid(query: &str, limit: u32, cursor: Option<&Cursor>) -> Result<(), String> {
    if query.encode_utf16().count() > 200
        || !(1..=100).contains(&limit)
        || cursor.is_some_and(|v| {
            v.query != query || !(0..=MAX_CLOCK).contains(&v.archived_at) || id(&v.uuid).is_err()
        })
    {
        return Err("ARCHIVE_INVALID_PAGE".into());
    }
    Ok(())
}
pub fn list(
    c: &mut Connection,
    query: &str,
    limit: u32,
    cursor: Option<&Cursor>,
) -> Result<Page, String> {
    query_valid(query, limit, cursor)?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    let stamp = cursor.map(|v| v.archived_at).unwrap_or(MAX_CLOCK);
    let uuid = cursor.map(|v| v.uuid.as_str()).unwrap_or("");
    let has = cursor.is_some();
    let keys = rows(
        &tx,
        "SELECT uuid FROM todos WHERE archived_at IS NOT NULL AND deleted_at IS NULL
        AND (instr(title,?1)>0 OR instr(COALESCE(note,''),?1)>0)
        AND (?2=0 OR archived_at<?3 OR (archived_at=?3 AND uuid>?4))
        ORDER BY archived_at DESC,uuid ASC LIMIT ?5",
        &[&query, &has, &stamp, &uuid, &(limit + 1)],
    )?;
    let mut items = Vec::new();
    for row in keys.iter().take(limit as usize) {
        items.push(read(&tx, row[0].as_deref().ok_or("ARCHIVE_INVALID_STATE")?)?.preview);
    }
    let next = if keys.len() > limit as usize {
        items.last().map(|v| Cursor {
            archived_at: v.archived_at,
            uuid: v.expected.uuid.clone(),
            query: query.into(),
        })
    } else {
        None
    };
    tx.commit().map_err(db)?;
    Ok(Page { items, next })
}
pub(crate) fn validate_expected(e: &Expected) -> Result<(), String> {
    id(&e.uuid)?;
    for value in [&e.scope, &e.fingerprint] {
        if value.len() != 64
            || !value
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("ARCHIVE_INVALID_SNAPSHOT".into());
        }
    }
    Ok(())
}
pub(crate) fn mutate(
    c: &Connection,
    e: &Expected,
    action: Action,
    now: i64,
    by: &str,
) -> Result<Outcome, String> {
    let current = read(c, &e.uuid)?;
    if current.preview.expected != *e {
        return Err("ARCHIVE_CONFLICT".into());
    }
    if current.rule_active {
        return Err("ARCHIVE_RULE_ACTIVE".into());
    }
    let next = now.max(
        current
            .preview
            .updated_at
            .checked_add(1)
            .ok_or("ARCHIVE_INVALID_VERSION")?,
    );
    version(next, by)?;
    let mut warnings = Vec::new();
    if action == Action::Delete {
        crate::task_note_link_store::tombstone_entity(c, &e.uuid, false, next, by).map_err(
            |e| {
                if e == "TASK_NOTE_LINK_CLOCK_EXHAUSTED" {
                    "ARCHIVE_INVALID_VERSION"
                } else {
                    "ARCHIVE_DATABASE_FAILED"
                }
            },
        )?;
        c.execute("UPDATE todos SET deleted_at=?1,reminder_at=NULL,updated_at=?1,updated_by=?2 WHERE uuid=?3",params![next,by,e.uuid]).map_err(db)?;
    } else {
        if !current.group_valid {
            let had: bool = c
                .query_row(
                    "SELECT group_uuid IS NOT NULL FROM todos WHERE uuid=?1",
                    [&e.uuid],
                    |r| r.get(0),
                )
                .map_err(db)?;
            if had {
                warnings.push("GROUP_RESET".into());
            }
        }
        c.execute(
            "UPDATE todos SET archived_at=NULL,reminder_at=NULL,updated_at=?1,updated_by=?2,
            group_uuid=CASE WHEN ?3 THEN group_uuid ELSE NULL END WHERE uuid=?4",
            params![next, by, current.group_valid, e.uuid],
        )
        .map_err(db)?;
        if action == Action::Reopen {
            c.execute("UPDATE todos SET completed=0,completed_at=NULL,repeat_rule=NULL,repeat_next_due_date=NULL,repeat_series_uuid=NULL,due_date=?1,due_at=?2 WHERE uuid=?3",
                params![current.preview.due_date,current.preview.due_at,e.uuid]).map_err(db)?;
        }
    }
    let revision: i64 = c
        .query_row(
            "SELECT todos_dirty_version FROM sync_runtime_state WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(db)?;
    if !(0..=MAX_CLOCK).contains(&revision) {
        return Err("ARCHIVE_INVALID_VERSION".into());
    }
    Ok(Outcome {
        uuid: e.uuid.clone(),
        action,
        outcome: "applied".into(),
        result_version: next,
        warnings,
    })
}
pub fn apply(
    c: &mut Connection,
    operation: &str,
    action: Action,
    e: &Expected,
    now: i64,
    by: &str,
) -> Result<Outcome, String> {
    id(operation)?;
    validate_expected(e)?;
    version(now, by)?;
    let tx = c
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(db)?;
    if scope(&tx)? != e.scope {
        return Err("ARCHIVE_SCOPE_CHANGED".into());
    }
    if meta(&tx, &format!("archive.batch.v1:{operation}"))?.is_some() {
        return Err("ARCHIVE_OPERATION_CONFLICT".into());
    }
    let request = hash(&json(&(action, e))?);
    let key = format!("archive.op.v1:{operation}");
    let result = if let Some(raw) = meta(&tx, &key)? {
        let mut receipt: Receipt =
            serde_json::from_str(&raw).map_err(|_| "ARCHIVE_INVALID_STATE")?;
        if receipt.request != request {
            return Err("ARCHIVE_OPERATION_CONFLICT".into());
        }
        receipt.result.outcome = "already_applied".into();
        receipt.result
    } else {
        let result = mutate(&tx, e, action, now, by)?;
        put(
            &tx,
            &key,
            &json(&Receipt {
                request,
                result: result.clone(),
            })?,
        )?;
        result
    };
    tx.commit().map_err(db)?;
    Ok(result)
}
