use super::*;
use uuid::Uuid;

fn db() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::db::configure_connection(&c).unwrap();
    crate::db::migrate(&mut c).unwrap();
    c
}
fn query() -> ReviewQuery {
    ReviewQuery {
        start_at: 100,
        end_at: 200,
        group_scope: "all".into(),
        group_uuid: None,
        keyword: String::new(),
    }
}
fn task(db: &Connection, title: &str) -> String {
    let id = Uuid::new_v4().to_string();
    db.execute("INSERT INTO todos(uuid,title,completed,created_at,updated_at,updated_by,sort_order) VALUES(?1,?2,0,1,1,'device',7)",params![id,title]).unwrap();
    id
}
fn entry(db: &Connection, parent: &str, at: i64, body: &str) -> String {
    let id = Uuid::new_v4().to_string();
    db.execute("INSERT INTO task_progress_entries(uuid,task_uuid,body,created_at,created_by,updated_at,updated_by,clock) VALUES(?1,?2,?3,?4,'device',?4,'device',1)",params![id,parent,body,at]).unwrap();
    id
}
fn group(db: &Connection, name: &str) -> String {
    let id = Uuid::new_v4().to_string();
    db.execute("INSERT INTO groups(uuid,name,color,sort_order,created_at,updated_at,updated_by) VALUES(?1,?2,'green',0,1,1,'device')",params![id,name]).unwrap();
    id
}

#[test]
fn boundaries_archive_completion_and_deleted_parents() {
    let mut c = db();
    let active = task(&c, "active");
    entry(&c, &active, 99, "before");
    entry(&c, &active, 100, "lower");
    entry(&c, &active, 199, "upper");
    entry(&c, &active, 200, "after");
    let archived = task(&c, "archive");
    entry(&c, &archived, 150, "retained");
    c.execute(
        "UPDATE todos SET completed=1,archived_at=160 WHERE uuid=?1",
        [&archived],
    )
    .unwrap();
    let trash = task(&c, "trash");
    entry(&c, &trash, 150, "hidden");
    c.execute("UPDATE todos SET deleted_at=160 WHERE uuid=?1", [&trash])
        .unwrap();
    entry(&c, &Uuid::new_v4().to_string(), 150, "orphan");
    let deleted = entry(&c, &active, 150, "gone");
    c.execute("UPDATE task_progress_entries SET body='',updated_at=170,deleted_at=170,clock=2 WHERE uuid=?1",[deleted]).unwrap();
    let page = list(&mut c, &query(), None).unwrap();
    assert_eq!(
        (page.matching_entry_count, page.matching_task_count),
        (3, 2)
    );
    assert_eq!(
        page.rows.iter().map(|r| r.created_at).collect::<Vec<_>>(),
        [199, 150, 100]
    );
    assert!(page.rows[1].archived && page.rows[1].completed);
    c.execute("UPDATE todos SET deleted_at=NULL WHERE uuid=?1", [&trash])
        .unwrap();
    assert_eq!(
        list(&mut c, &query(), None).unwrap().matching_entry_count,
        4
    );
    c.execute("INSERT INTO lifecycle_terminals(kind,uuid,operation_uuid,purged_at) VALUES('todo',?1,?2,200)",params![trash,Uuid::new_v4().to_string()]).unwrap();
    assert_eq!(snapshot(&mut c, &query()).unwrap().matching_entry_count, 3);
}

#[test]
fn current_group_and_literal_ascii_search() {
    let mut c = db();
    let id = task(&c, "中文 ABC 100%_\\");
    let g = group(&c, "工作");
    c.execute(
        "UPDATE todos SET group_uuid=?1 WHERE uuid=?2",
        params![g, id],
    )
    .unwrap();
    entry(&c, &id, 150, "中文 进展 %_\\ İ Ä");
    let other = task(&c, "other");
    entry(&c, &other, 150, "none");
    for key in ["abc", " ABC ", "中文", "%", "_", "\\", "%_\\", "Ä", "İ"] {
        let mut q = query();
        q.keyword = key.into();
        assert_eq!(
            list(&mut c, &q, None).unwrap().matching_entry_count,
            1,
            "{key}"
        );
    }
    for key in ["ä", "i", "' OR 1=1 --", "%nothing%"] {
        let mut q = query();
        q.keyword = key.into();
        assert_eq!(
            list(&mut c, &q, None).unwrap().matching_entry_count,
            0,
            "{key}"
        );
    }
    let mut q = query();
    q.group_scope = "group".into();
    q.group_uuid = Some(g.clone());
    assert_eq!(
        snapshot(&mut c, &q).unwrap().rows[0].group_name.as_deref(),
        Some("工作")
    );
    c.execute("UPDATE groups SET deleted_at=180 WHERE uuid=?1", [g])
        .unwrap();
    assert_eq!(list(&mut c, &q, None).unwrap().matching_entry_count, 0);
    q.group_scope = "ungrouped".into();
    q.group_uuid = None;
    assert_eq!(list(&mut c, &q, None).unwrap().matching_entry_count, 2);
}

#[test]
fn paging_is_stable_bound_to_query_and_detects_parent_edits() {
    let mut c = db();
    let id = task(&c, "task");
    for _ in 0..65 {
        entry(&c, &id, 150, "progress");
    }
    let first = list(&mut c, &query(), None).unwrap();
    assert_eq!(first.rows.len(), 30);
    let cursor = first.next_cursor.as_ref().unwrap();
    let second = list(&mut c, &query(), Some(cursor)).unwrap();
    let third = list(&mut c, &query(), second.next_cursor.as_ref()).unwrap();
    assert_eq!((second.rows.len(), third.rows.len()), (30, 5));
    assert!(third.next_cursor.is_none());
    let all = snapshot(&mut c, &query()).unwrap();
    let merged = first
        .rows
        .iter()
        .chain(&second.rows)
        .chain(&third.rows)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(merged, all.rows);
    let mut different = query();
    different.keyword = "other".into();
    assert_eq!(
        list(&mut c, &different, Some(cursor)).unwrap_err(),
        "REVIEW_INVALID"
    );
    assert!(validate(&mut c, &query(), &all.snapshot_token).unwrap());
    // A rename can happen without advancing the progress revision or the wall-clock timestamp.
    c.execute("UPDATE todos SET title='new title' WHERE uuid=?1", [id])
        .unwrap();
    assert!(!validate(&mut c, &query(), &all.snapshot_token).unwrap());
    assert_eq!(
        list(&mut c, &query(), Some(cursor)).unwrap_err(),
        "REVIEW_CHANGED"
    );
}

#[test]
fn progress_edit_keeps_created_date_but_invalidates_copy() {
    let mut c = db();
    let id = task(&c, "task");
    let record = entry(&c, &id, 150, "before");
    let before = snapshot(&mut c, &query()).unwrap();
    c.execute(
        "UPDATE task_progress_entries SET body='new content',updated_at=500,clock=2 WHERE uuid=?1",
        [record],
    )
    .unwrap();
    let after = snapshot(&mut c, &query()).unwrap();
    assert_eq!(after.rows[0].created_at, 150);
    assert_eq!(after.rows[0].updated_at, 500);
    assert!(!validate(&mut c, &query(), &before.snapshot_token).unwrap());
    assert!(validate(&mut c, &query(), &after.snapshot_token).unwrap());
}

#[test]
fn identical_data_on_a_changed_sync_target_invalidates_copy_and_cursor() {
    let mut c = db();
    let id = task(&c, "task");
    for _ in 0..31 {
        entry(&c, &id, 150, "record");
    }
    c.execute(
        "INSERT INTO app_metadata(key,value) VALUES('sync.target.epoch.v1','first')",
        [],
    )
    .unwrap();
    let page = list(&mut c, &query(), None).unwrap();
    c.execute(
        "UPDATE app_metadata SET value='second' WHERE key='sync.target.epoch.v1'",
        [],
    )
    .unwrap();
    assert!(!validate(&mut c, &query(), &page.snapshot_token).unwrap());
    assert_eq!(
        list(&mut c, &query(), page.next_cursor.as_ref()).unwrap_err(),
        "REVIEW_CHANGED"
    );
}

#[test]
fn read_only_and_invalid_inputs_do_not_change_database() {
    let mut c = db();
    let id = task(&c, "task");
    entry(&c, &id, 150, "progress");
    let before = c.total_changes();
    let page = list(&mut c, &query(), None).unwrap();
    snapshot(&mut c, &query()).unwrap();
    validate(&mut c, &query(), &page.snapshot_token).unwrap();
    assert_eq!(c.total_changes(), before);
    let mut invalid = query();
    invalid.end_at = invalid.start_at;
    assert_eq!(list(&mut c, &invalid, None).unwrap_err(), "REVIEW_INVALID");
    invalid = query();
    invalid.keyword = "😀".repeat(51);
    assert_eq!(snapshot(&mut c, &invalid).unwrap_err(), "REVIEW_INVALID");
    invalid = query();
    invalid.keyword = "x\0y".into();
    assert_eq!(snapshot(&mut c, &invalid).unwrap_err(), "REVIEW_INVALID");
    invalid = query();
    invalid.group_scope = "group".into();
    invalid.group_uuid = Some("bad".into());
    assert_eq!(list(&mut c, &invalid, None).unwrap_err(), "REVIEW_INVALID");
    assert_eq!(
        validate(&mut c, &query(), "invalid").unwrap_err(),
        "REVIEW_INVALID"
    );
}

#[test]
fn utf16_copy_budget_does_not_limit_browsing() {
    let mut c = db();
    let id = task(&c, "task");
    for _ in 0..101 {
        entry(&c, &id, 150, &"😀".repeat(500));
    }
    assert_eq!(snapshot(&mut c, &query()).unwrap_err(), "REVIEW_LIMIT");
    let page = list(&mut c, &query(), None).unwrap();
    assert_eq!(page.rows.len(), 30);
    assert_eq!(page.matching_entry_count, 101);
}

#[test]
fn bounded_domain_real_sql_performance() {
    for n in [1000, 10000, 20000] {
        let mut c = db();
        let id = task(&c, "task");
        let tx = c.transaction().unwrap();
        for _ in 0..n {
            entry(&tx, &id, 150, "推进");
        }
        tx.commit().unwrap();
        let before = c.total_changes();
        let started = std::time::Instant::now();
        let first = list(&mut c, &query(), None).unwrap();
        let list_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let second = list(&mut c, &query(), first.next_cursor.as_ref()).unwrap();
        let page_ms = started.elapsed().as_millis();
        let mut filtered = query();
        filtered.keyword = "推进".into();
        let started = std::time::Instant::now();
        let found = list(&mut c, &filtered, None).unwrap();
        let search_ms = started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let all = snapshot(&mut c, &query()).unwrap();
        let snapshot_ms = started.elapsed().as_millis();
        assert_eq!(found.matching_entry_count, n);
        assert_eq!(all.rows.len(), n as usize);
        assert_eq!(second.rows.len(), 30);
        assert_eq!(c.total_changes(), before);
        println!("work-review real SQLite {n}: first={list_ms}ms page={page_ms}ms search={search_ms}ms snapshot={snapshot_ms}ms (debug)");
    }
}

#[test]
fn shared_fixture_matches_real_sql_rows_and_keyword_cases() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/fixtures/work-review-v1.json")).unwrap();
    let mut c = db();
    let expected: Vec<ReviewRow> =
        serde_json::from_value(fixture["snapshot"]["rows"].clone()).unwrap();
    let mut parents = std::collections::HashSet::new();
    for r in &expected {
        if parents.insert(r.task_uuid.clone()) {
            c.execute("INSERT INTO todos(uuid,title,completed,archived_at,created_at,updated_at,updated_by,sort_order) VALUES(?1,?2,?3,?4,1,1,'fixture',0)",
                params![r.task_uuid,r.task_title,r.completed,r.archived.then_some(1)]).unwrap();
        }
        c.execute("INSERT INTO task_progress_entries(uuid,task_uuid,body,created_at,created_by,updated_at,updated_by,clock) VALUES(?1,?2,?3,?4,'fixture',?5,'fixture',1)",
            params![r.record_uuid,r.task_uuid,r.body,r.created_at,r.updated_at]).unwrap();
    }
    let mut q: ReviewQuery = serde_json::from_value(fixture["query"].clone()).unwrap();
    let result = snapshot(&mut c, &q).unwrap();
    assert_eq!(result.rows, expected);
    assert_eq!(
        (result.matching_entry_count, result.matching_task_count),
        (3, 2)
    );
    for case in fixture["keyword_cases"].as_array().unwrap() {
        q.keyword = case["input"].as_str().unwrap().into();
        let got = list(&mut c, &q, None).unwrap();
        let ids: Vec<&str> = got.rows.iter().map(|r| r.record_uuid.as_str()).collect();
        let expected: Vec<&str> = case["record_uuids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(ids, expected);
        assert_eq!(
            normalized(&q).unwrap().keyword,
            case["normalized"].as_str().unwrap()
        );
    }
}
