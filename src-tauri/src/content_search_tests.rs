use crate::content_search::{self, SearchScope};
use rusqlite::{params, Connection};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    setup_sql: Vec<String>,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    id: String,
    scope: SearchScope,
    query: String,
    offset: u32,
    limit: u32,
    total: i64,
    expected: Vec<String>,
    error: Option<String>,
    first_field: Option<String>,
    first_archived: Option<bool>,
    parent_uuid: Option<String>,
    excerpt_contains: Option<String>,
    normalized: Option<String>,
}
fn fixture() -> (Connection, Fixture) {
    let fixture: Fixture =
        serde_json::from_str(include_str!("../../docs/fixtures/content-search-v1.json")).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::db::migrate(&mut db).unwrap();
    for sql in &fixture.setup_sql {
        db.execute_batch(sql).unwrap();
    }
    (db, fixture)
}

#[test]
fn content_search_shared_contract() {
    let (db, fixture) = fixture();
    let changes = db
        .query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
        .unwrap();
    for case in fixture.cases {
        let result = content_search::search(&db, case.scope, &case.query, case.offset, case.limit);
        if let Some(error) = case.error {
            assert_eq!(result.unwrap_err(), error, "{}", case.id);
            continue;
        }
        let page = result.unwrap();
        assert_eq!(page.total, case.total, "{}", case.id);
        assert_eq!(
            page.items
                .iter()
                .map(|i| i.uuid.clone())
                .collect::<Vec<_>>(),
            case.expected,
            "{}",
            case.id
        );
        assert_eq!(
            (page.scope, page.offset, page.limit),
            (case.scope, case.offset, case.limit)
        );
        assert!(page.items.iter().all(|i| i.excerpt.chars().count() <= 160));
        if let Some(q) = case.normalized {
            assert_eq!(page.query, q);
        }
        if let Some(field) = case.first_field {
            assert_eq!(page.items[0].matched_field, field);
        }
        if let Some(archived) = case.first_archived {
            assert_eq!(page.items[0].archived, archived);
            assert!(page.items[0].completed);
        }
        if let Some(parent) = case.parent_uuid {
            assert_eq!(page.items[0].parent_uuid.as_deref(), Some(parent.as_str()));
        }
        if let Some(text) = case.excerpt_contains {
            assert!(page.items[0].excerpt.contains(&text));
        }
    }
    assert_eq!(
        db.query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        changes
    );
}

#[test]
fn content_search_resolves_current_destinations_and_excludes_deleted_parents() {
    let (db, _) = fixture();
    let uuid = "123e4567-e89b-42d3-a456-200000000001";
    let asset = "123e4567-e89b-42d3-a456-300000000001";
    assert!(
        content_search::resolve(
            &db,
            SearchScope::Todo,
            "123e4567-e89b-42d3-a456-100000000003"
        )
        .unwrap()
        .archived
    );
    let found = content_search::search(&db, SearchScope::Attachment, "needle", 0, 20).unwrap();
    assert_eq!(found.items[0].uuid, asset);
    db.execute(
        "UPDATE notes SET content='new current text' WHERE uuid=?",
        [uuid],
    )
    .unwrap();
    let opened = content_search::resolve(&db, SearchScope::Attachment, asset).unwrap();
    assert_eq!(opened.content, "new current text");
    assert_eq!(opened.parent_uuid.as_deref(), Some(uuid));
    let wire = serde_json::to_string(&opened).unwrap();
    for field in ["local_original_path", "sha256", "transfer_error"] {
        assert!(!wire.contains(field));
    }
    db.execute("UPDATE notes SET deleted_at=999 WHERE uuid=?", [uuid])
        .unwrap();
    assert_eq!(
        content_search::resolve(&db, SearchScope::Attachment, asset).unwrap_err(),
        "SEARCH_UNAVAILABLE"
    );
    assert_eq!(
        content_search::resolve(&db, SearchScope::Note, uuid).unwrap_err(),
        "SEARCH_UNAVAILABLE"
    );
    assert_eq!(
        content_search::resolve(
            &db,
            SearchScope::Attachment,
            "123e4567-e89b-42d3-a456-300000000004"
        )
        .unwrap_err(),
        "SEARCH_UNAVAILABLE"
    );
    assert_eq!(
        content_search::resolve(&db, SearchScope::Todo, "' OR 1=1").unwrap_err(),
        "SEARCH_INVALID_ID"
    );
    db.execute_batch("DROP TABLE note_attachments").unwrap();
    assert_eq!(
        content_search::search(&db, SearchScope::Attachment, "needle", 0, 20).unwrap_err(),
        "SEARCH_DATABASE_FAILED"
    );
}

#[test]
fn content_search_orders_pages_and_bounds_large_documents() {
    let (db, _) = fixture();
    let text = format!("{}deepmatch{}", "x".repeat(19000), "y".repeat(990));
    for index in 0..120 {
        db.execute("INSERT INTO notes(uuid,title,content,created_at,updated_at,updated_by) VALUES(?1,'page item',?2,1,10,'fixture')",
            params![format!("123e4567-e89b-42d3-a456-9{index:011}"), text]).unwrap();
    }
    let start = std::time::Instant::now();
    let first = content_search::search(&db, SearchScope::Note, "deepmatch", 0, 50).unwrap();
    let second = content_search::search(&db, SearchScope::Note, "deepmatch", 50, 50).unwrap();
    let third = content_search::search(&db, SearchScope::Note, "deepmatch", 100, 50).unwrap();
    assert_eq!(
        (
            first.total,
            first.items.len(),
            second.items.len(),
            third.items.len()
        ),
        (120, 50, 50, 20)
    );
    let all: Vec<_> = first
        .items
        .iter()
        .chain(&second.items)
        .chain(&third.items)
        .collect();
    assert!(all.windows(2).all(|pair| pair[0].uuid < pair[1].uuid));
    assert!(all
        .iter()
        .all(|item| item.excerpt.contains("deepmatch") && item.excerpt.chars().count() <= 160));
    println!(
        "content search: 120 near-limit documents, 3 pages in {:?}",
        start.elapsed()
    );
}
