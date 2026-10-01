use super::*;
use crate::task_progress_store::{self as store, ProgressWrite};
use uuid::Uuid;

fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut c).unwrap();
    c
}
fn task(c: &Connection) -> String {
    let id = Uuid::new_v4().to_string();
    c.execute("INSERT INTO todos(uuid,title,completed,created_at,updated_at,updated_by,sort_order) VALUES(?1,'task',0,1,1,'device',7)",[&id]).unwrap();
    id
}
fn request(task: &str) -> ProgressWrite {
    ProgressWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: task.into(),
        record_uuid: Uuid::new_v4().to_string(),
        action: "create".into(),
        body: "local".into(),
        expected_record: None,
    }
}
fn epoch(c: &Connection) -> String {
    sync_target::capture(c).unwrap()
}
fn operations(c: &Connection) -> Vec<(i64, i64)> {
    c.prepare(
        "SELECT write_revision,published FROM task_progress_operations ORDER BY write_revision",
    )
    .unwrap()
    .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}
const EMPTY: &str = r#"{"format_version":1,"entries":[]}"#;

#[test]
fn ack_exact_snapshot_publishes_only_sent_receipts_and_keeps_late_write_dirty() {
    let mut c = db();
    let id = task(&c);
    let epoch = epoch(&c);
    store::write(&mut c, &request(&id), 100, "device").unwrap();
    let sent = prepare(&mut c, &epoch, None, None).unwrap();
    assert_eq!(
        acknowledge(&mut c, &sent, None).unwrap_err(),
        "PROGRESS_NOT_EMPTY"
    );
    store::write(&mut c, &request(&id), 101, "device").unwrap();
    assert!(!is_current(&mut c, &sent).unwrap());
    assert!(!acknowledge(&mut c, &sent, Some("\"uploaded\"")).unwrap());
    assert_eq!(operations(&c), vec![(1, 1), (2, 0)]);
    let s = store::snapshot(&mut c).unwrap();
    assert_eq!(s.synced_revision, 0);
    assert_eq!(s.etag.as_deref(), Some("\"uploaded\""));
    let now = prepare(&mut c, &epoch, Some(&sent.document), Some("\"uploaded\"")).unwrap();
    assert!(acknowledge(&mut c, &now, Some("\"next\"")).unwrap());
    let s = store::snapshot(&mut c).unwrap();
    assert_eq!(s.synced_revision, s.revision);
    assert!(operations(&c).iter().all(|(_, p)| *p == 1));
    assert!(!acknowledge(&mut c, &sent, Some("\"old-upload\"")).unwrap());
    assert_eq!(
        store::snapshot(&mut c).unwrap().etag.as_deref(),
        Some("\"next\"")
    );
}

#[test]
fn overwrite_notice_requires_exact_pending_current_record_token() {
    let mut c = db();
    let id = task(&c);
    let epoch = epoch(&c);
    let r = request(&id);
    let page = store::write(&mut c, &r, 100, "device").unwrap();
    let view = &page.entries[0];
    let edit = ProgressWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        action: "edit".into(),
        body: "local v2".into(),
        expected_record: Some(view.token.clone()),
        ..r.clone()
    };
    store::write(&mut c, &edit, 101, "device").unwrap();
    // A losing old remote edit cannot warn about the pending v2.
    let old = protocol::Document {
        format_version: 1,
        entries: vec![view.record.clone()],
    };
    store::restore(&mut c, &old).unwrap();
    assert!(!store::list(&mut c, &id, None).unwrap().overwritten);
    let mut remote = store::snapshot(&mut c).unwrap().document;
    remote.entries[0].clock += 1;
    remote.entries[0].body = "remote wins".into();
    prepare(
        &mut c,
        &epoch,
        Some(&protocol::encode(&remote).unwrap()),
        Some("\"remote\""),
    )
    .unwrap();
    assert!(store::list(&mut c, &id, None).unwrap().overwritten);
    store::dismiss_notice(&c, &id).unwrap();
    assert!(!store::list(&mut c, &id, None).unwrap().overwritten);
    // Old unpublished receipts no longer reference the current remote version.
    remote.entries[0].clock += 1;
    remote.entries[0].body = "remote v2".into();
    store::restore(&mut c, &remote).unwrap();
    assert!(!store::list(&mut c, &id, None).unwrap().overwritten);
}

#[test]
fn published_edit_displaced_by_remote_does_not_notice_but_pending_delete_does() {
    let mut c = db();
    let id = task(&c);
    let epoch = epoch(&c);
    store::write(&mut c, &request(&id), 100, "device").unwrap();
    let sent = prepare(&mut c, &epoch, None, None).unwrap();
    assert!(acknowledge(&mut c, &sent, Some("\"ack\"")).unwrap());
    let mut remote = store::snapshot(&mut c).unwrap().document;
    remote.entries[0].clock += 1;
    remote.entries[0].body = "remote".into();
    store::restore(&mut c, &remote).unwrap();
    assert!(!store::list(&mut c, &id, None).unwrap().overwritten);
    let page = store::list(&mut c, &id, None).unwrap();
    let v = &page.entries[0];
    let edit = ProgressWrite {
        operation_uuid: Uuid::new_v4().to_string(),
        task_uuid: id.clone(),
        record_uuid: v.record.uuid.clone(),
        action: "edit".into(),
        body: "pending".into(),
        expected_record: Some(v.token.clone()),
    };
    store::write(&mut c, &edit, 101, "device").unwrap();
    // Delete dominates this pending edit despite its lower clock.
    remote.entries[0].body.clear();
    remote.entries[0].deleted_at = Some(remote.entries[0].updated_at);
    store::restore(&mut c, &remote).unwrap();
    assert!(store::list(&mut c, &id, None).unwrap().overwritten);
}

#[test]
fn generation_and_target_stale_acks_cannot_publish_receipts_or_etags() {
    let mut c = db();
    let id = task(&c);
    let epoch = epoch(&c);
    store::write(&mut c, &request(&id), 100, "device").unwrap();
    let sent = prepare(&mut c, &epoch, None, None).unwrap();
    c.execute(
        "UPDATE task_progress_sync_state SET generation=generation+1",
        [],
    )
    .unwrap();
    assert!(!acknowledge(&mut c, &sent, Some("\"stale-generation\"")).unwrap());
    assert_eq!(operations(&c), vec![(1, 0)]);
    assert!(store::snapshot(&mut c).unwrap().etag.is_none());
    let sent = prepare(&mut c, &epoch, None, None).unwrap();
    c.execute(
        "UPDATE app_metadata SET value='new-epoch' WHERE key='sync.target.epoch.v1'",
        [],
    )
    .unwrap();
    assert!(!acknowledge(&mut c, &sent, Some("\"stale-target\"")).unwrap());
    assert_eq!(operations(&c), vec![(1, 0)]);
    assert!(store::snapshot(&mut c).unwrap().etag.is_none());
    assert_eq!(
        prepare(&mut c, &epoch, None, None).unwrap_err(),
        "PROGRESS_CONFIG_CHANGED"
    );
}

#[test]
fn remote_seen_survives_failed_merge_and_missing_object_cannot_be_recreated() {
    let mut c = db();
    let id = task(&c);
    let epoch = epoch(&c);
    store::write(&mut c, &request(&id), 100, "device").unwrap();
    let before = store::snapshot(&mut c).unwrap();
    let mut bad = before.document.clone();
    bad.entries[0].created_by = "conflict".into();
    assert_eq!(
        prepare(
            &mut c,
            &epoch,
            Some(&protocol::encode(&bad).unwrap()),
            Some("\"seen\"")
        )
        .unwrap_err(),
        "PROGRESS_CONFLICT"
    );
    assert_eq!(store::snapshot(&mut c).unwrap(), before);
    assert!(remote_seen(&c, &epoch).unwrap());
    assert_eq!(
        prepare(&mut c, &epoch, None, None).unwrap_err(),
        "PROGRESS_REMOTE_MISSING"
    );
}

#[test]
fn failed_ack_rolls_back_seen_etag_publication_and_revision() {
    let mut c = db();
    let id = task(&c);
    let epoch = epoch(&c);
    store::write(&mut c, &request(&id), 100, "device").unwrap();
    let sent = prepare(&mut c, &epoch, None, None).unwrap();
    c.execute_batch("CREATE TRIGGER fail_progress_ack BEFORE UPDATE OF synced_revision ON task_progress_sync_state BEGIN SELECT RAISE(ABORT,'failure'); END").unwrap();
    assert_eq!(
        acknowledge(&mut c, &sent, Some("\"uploaded\"")).unwrap_err(),
        "PROGRESS_DATABASE"
    );
    assert_eq!(operations(&c), vec![(1, 0)]);
    let s = store::snapshot(&mut c).unwrap();
    assert_eq!(s.synced_revision, 0);
    assert!(s.etag.is_none());
    assert!(!remote_seen(&c, &epoch).unwrap());
}

#[test]
fn exact_document_is_part_of_receipt_and_empty_first_contact_needs_no_etag() {
    let mut c = db();
    let epoch = epoch(&c);
    let empty = prepare(&mut c, &epoch, None, None).unwrap();
    assert_eq!(empty.document, EMPTY);
    assert!(acknowledge(&mut c, &empty, None).unwrap());
    let id = task(&c);
    store::write(&mut c, &request(&id), 100, "device").unwrap();
    let sent = prepare(&mut c, &epoch, None, None).unwrap();
    // Simulate a corrupted snapshot: revision alone must never be sufficient.
    let mut changed = sent.clone();
    changed.document = EMPTY.into();
    assert!(!is_current(&mut c, &changed).unwrap());
    assert!(!acknowledge(&mut c, &changed, Some("\"not-current\"")).unwrap());
    assert_eq!(store::snapshot(&mut c).unwrap().synced_revision, 0);
    assert_eq!(operations(&c), vec![(1, 0)]);
    assert!(store::snapshot(&mut c).unwrap().etag.is_none());
}
