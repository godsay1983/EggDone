use super::*;
use serde::Deserialize;
use serde_json::{json, value::RawValue, Value};

#[derive(Deserialize)]
struct RawCase {
    raw: String,
    expected: Box<RawValue>,
}

#[derive(Deserialize)]
struct MergeCase {
    name: String,
    left: Box<RawValue>,
    right: Box<RawValue>,
    expected: Box<RawValue>,
}

#[derive(Deserialize)]
struct ConflictCase {
    left: Box<RawValue>,
    right: Box<RawValue>,
}

#[derive(Deserialize)]
struct LawCase {
    a: Box<RawValue>,
    b: Box<RawValue>,
    c: Box<RawValue>,
}

#[derive(Deserialize)]
struct PurgeCase {
    document: Box<RawValue>,
    tasks: HashSet<String>,
    expected: Box<RawValue>,
}

#[derive(Deserialize)]
struct KeyCase {
    todo: String,
    expected: String,
}

#[derive(Deserialize)]
struct CollisionCase {
    todo: String,
    occupied: Vec<String>,
}

#[derive(Deserialize)]
struct Fixtures {
    valid: Vec<Box<RawValue>>,
    invalid: Vec<Box<RawValue>>,
    valid_raw: Vec<RawCase>,
    invalid_raw: Vec<String>,
    merges: Vec<MergeCase>,
    conflicts: Vec<ConflictCase>,
    laws: Vec<LawCase>,
    purges: Vec<PurgeCase>,
    keys: Vec<KeyCase>,
    bad_keys: Vec<String>,
    key_collisions: Vec<CollisionCase>,
}

fn fixtures() -> Fixtures {
    // RawValue preserves intentionally unpaired surrogates in invalid documents.
    serde_json::from_str(include_str!("../../docs/fixtures/task-progress-v1.json")).unwrap()
}

fn document(raw: &RawValue) -> Document {
    parse(raw.get()).unwrap()
}

fn entry(index: usize) -> Entry {
    Entry {
        uuid: format!("11111111-1111-4111-8111-{index:012x}"),
        task_uuid: "11111111-1111-4111-8111-000000000010".into(),
        body: "text".into(),
        created_at: 0,
        created_by: "desktop".into(),
        updated_at: 1,
        updated_by: "desktop".into(),
        clock: 1,
        deleted_at: None,
    }
}

fn rows(start: usize, count: usize, body: &str) -> Document {
    Document {
        format_version: 1,
        entries: (start..start + count)
            .map(|index| Entry {
                body: body.into(),
                ..entry(index)
            })
            .collect(),
    }
}

#[test]
fn shared_documents_and_raw_vectors() {
    let f = fixtures();
    assert!(!f.valid.is_empty() && !f.invalid.is_empty());
    assert!(!f.valid_raw.is_empty() && !f.invalid_raw.is_empty());
    for raw in f.valid {
        let doc = document(&raw);
        validate(&doc).unwrap();
        let encoded = encode(&doc).unwrap();
        assert_eq!(parse(&encoded).unwrap(), doc);
        assert_eq!(merge(&doc, &doc).unwrap(), doc);
    }
    for (index, raw) in f.invalid.iter().enumerate() {
        assert_eq!(parse(raw.get()).unwrap_err(), "PROGRESS_INVALID", "{index}");
    }
    for case in f.valid_raw {
        assert_eq!(parse(&case.raw).unwrap(), document(&case.expected));
    }
    for raw in f.invalid_raw {
        assert_eq!(parse(&raw).unwrap_err(), "PROGRESS_INVALID", "{raw}");
    }
}

#[test]
fn shared_merge_conflict_and_law_vectors() {
    let f = fixtures();
    assert!(!f.merges.is_empty() && !f.conflicts.is_empty() && !f.laws.is_empty());
    for case in f.merges {
        let left = document(&case.left);
        let right = document(&case.right);
        let expected = document(&case.expected);
        assert_eq!(merge(&left, &right).unwrap(), expected, "{}", case.name);
        assert_eq!(merge(&right, &left).unwrap(), expected, "{}", case.name);
        assert_eq!(merge(&expected, &left).unwrap(), expected);
        assert_eq!(merge(&expected, &right).unwrap(), expected);
    }
    for case in f.conflicts {
        let left = document(&case.left);
        let right = document(&case.right);
        let originals = (left.clone(), right.clone());
        assert_eq!(merge(&left, &right).unwrap_err(), "PROGRESS_CONFLICT");
        assert_eq!(merge(&right, &left).unwrap_err(), "PROGRESS_CONFLICT");
        assert_eq!((left, right), originals);
    }
    for case in f.laws {
        let docs = [document(&case.a), document(&case.b), document(&case.c)];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let [a, b, c] = order.map(|index| &docs[index]);
            assert_eq!(merge(a, a).unwrap(), *a);
            assert_eq!(merge(a, b).unwrap(), merge(b, a).unwrap());
            assert_eq!(
                merge(&merge(a, b).unwrap(), c).unwrap(),
                merge(a, &merge(b, c).unwrap()).unwrap()
            );
        }
    }
}

#[test]
fn shared_purge_and_key_vectors() {
    let f = fixtures();
    assert!(!f.purges.is_empty() && !f.keys.is_empty());
    assert!(!f.bad_keys.is_empty() && !f.key_collisions.is_empty());
    for case in f.purges {
        let original = document(&case.document);
        let result = filter_purged(&original, &case.tasks).unwrap();
        assert_eq!(result, document(&case.expected));
        assert_eq!(filter_purged(&result, &case.tasks).unwrap(), result);
        assert_eq!(original, document(&case.document));
    }
    for case in f.keys {
        assert_eq!(object_key(&case.todo, &[]).unwrap(), case.expected);
    }
    for todo in f.bad_keys {
        assert_eq!(object_key(&todo, &[]).unwrap_err(), "PROGRESS_KEY_INVALID");
    }
    for case in f.key_collisions {
        assert_eq!(
            object_key(&case.todo, &case.occupied).unwrap_err(),
            "PROGRESS_KEY_COLLISION"
        );
    }
}

#[test]
fn canonical_field_order_and_uuid_order() {
    assert_eq!(
        encode(&Document::default()).unwrap(),
        r#"{"format_version":1,"entries":[]}"#
    );
    let mut doc = rows(1, 2, "text");
    let expected = serde_json::to_string(&doc).unwrap();
    doc.entries.reverse();
    assert_eq!(encode(&doc).unwrap(), expected);
    assert_eq!(parse(&expected).unwrap().entries[0].uuid, entry(1).uuid);
    assert_eq!(
        serde_json::to_string(&entry(1)).unwrap(),
        r#"{"uuid":"11111111-1111-4111-8111-000000000001","task_uuid":"11111111-1111-4111-8111-000000000010","body":"text","created_at":0,"created_by":"desktop","updated_at":1,"updated_by":"desktop","clock":1,"deleted_at":null}"#
    );
}

#[test]
fn exact_ecmascript_trim_and_banned_controls() {
    let whitespace: Vec<u32> = (0x0009..=0x000d)
        .chain([0x0020, 0x00a0, 0x1680])
        .chain(0x2000..=0x200a)
        .chain([0x2028, 0x2029, 0x202f, 0x205f, 0x3000, 0xfeff])
        .collect();
    for point in whitespace {
        let c = char::from_u32(point).unwrap();
        let body = format!("{c}a\r\nb\tc{c}");
        assert_eq!(normalize_body(&body).unwrap(), "a\r\nb\tc", "{point:x}");
        assert_eq!(
            normalize_body(&c.to_string()).unwrap_err(),
            "PROGRESS_INVALID"
        );
        let doc = rows(1, 1, &body);
        assert_eq!(validate(&doc).unwrap_err(), "PROGRESS_INVALID");
    }
    for point in (0..=31)
        .filter(|n| !matches!(n, 9 | 10 | 13))
        .chain(127..=159)
        .chain(0x202a..=0x202e)
        .chain(0x2066..=0x2069)
    {
        let body = format!("a{}b", char::from_u32(point).unwrap());
        assert_eq!(
            normalize_body(&body).unwrap_err(),
            "PROGRESS_INVALID",
            "{point:x}"
        );
    }
    for point in [0x180e, 0x200b, 0x2060] {
        let body = format!(
            "{}a{}",
            char::from_u32(point).unwrap(),
            char::from_u32(point).unwrap()
        );
        assert_eq!(normalize_body(&body).unwrap(), body);
    }
    assert_eq!(normalize_body("e\u{301}").unwrap(), "e\u{301}");
}

#[test]
fn utf16_body_boundaries() {
    for body in ["a".repeat(1000), "\u{10000}".repeat(500)] {
        assert_eq!(normalize_body(&body).unwrap(), body);
        validate(&rows(1, 1, &body)).unwrap();
        let oversized = format!("{body}a");
        assert_eq!(normalize_body(&oversized).unwrap_err(), "PROGRESS_LIMIT");
        assert_eq!(
            validate(&rows(1, 1, &oversized)).unwrap_err(),
            "PROGRESS_LIMIT"
        );
    }
}

#[test]
fn integer_notation_and_rounding_security() {
    let doc = rows(1, 1, "text 1e0 \\\"clock\\\":1");
    let raw = encode(&doc).unwrap();
    for token in ["1", "1.0", "1e0", "10e-1", "0.001e3", "1000.000e-3", "1E+0"] {
        assert_eq!(
            parse(&raw.replace("\"clock\":1", &format!("\"clock\":{token}"))).unwrap(),
            doc
        );
    }
    for token in [
        "-1",
        "0",
        "1.1",
        "true",
        "\"1\"",
        "null",
        "01",
        "1.",
        "1e",
        "+1",
        "9007199254740992",
        "9007199254740991.1",
        "9007199254740990.9",
        "9.0071992547409911e15",
        "1.00000000000000000001",
        "1.00000000000000001",
        "-1e-999",
        "1e-999",
        "1e999",
        "NaN",
        "Infinity",
    ] {
        let source = raw.replace("\"clock\":1", &format!("\"clock\":{token}"));
        assert_eq!(parse(&source).unwrap_err(), "PROGRESS_INVALID", "{token}");
    }
    let maximum = raw.replace("\"clock\":1", "\"clock\":9007199254740991.0");
    assert_eq!(parse(&maximum).unwrap().entries[0].clock, MAX_CLOCK);
    let exponent = raw.replace("\"clock\":1", "\"clock\":90071992547409910e-1");
    assert_eq!(parse(&exponent).unwrap().entries[0].clock, MAX_CLOCK);
    assert_eq!(next_clock(0).unwrap(), 1);
    assert_eq!(next_clock(1).unwrap(), 2);
    assert_eq!(next_clock(MAX_CLOCK - 1).unwrap(), MAX_CLOCK);
    assert_eq!(next_clock(MAX_CLOCK).unwrap_err(), "PROGRESS_LIMIT");
    for clock in [-1, MAX_CLOCK + 1, i64::MAX] {
        assert_eq!(next_clock(clock).unwrap_err(), "PROGRESS_INVALID");
    }
}

#[test]
fn required_nullable_duplicate_fields_and_invalid_types() {
    let mut value = serde_json::to_value(rows(1, 1, "text")).unwrap();
    value["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("deleted_at");
    assert_eq!(parse(&value.to_string()).unwrap_err(), "PROGRESS_INVALID");
    assert!(serde_json::from_value::<Document>(value).is_err());
    let raw = encode(&rows(1, 1, "text")).unwrap();
    for source in [
        raw.replace(
            "\"format_version\":1",
            "\"format_version\":1,\"format_version\":1",
        ),
        raw.replace(
            "\"deleted_at\":null",
            "\"deleted_at\":null,\"deleted_at\":null",
        ),
        raw.replace("\"clock\":1", "\"clock\":1,\"clock\":1"),
        raw.replace("\"body\":\"text\"", r#""body":"\ud800""#),
        raw.replace("\"body\":\"text\"", r#""body":"\udfff""#),
    ] {
        assert_eq!(parse(&source).unwrap_err(), "PROGRESS_INVALID");
    }
    for field in ["created_at", "updated_at", "clock", "deleted_at"] {
        for wrong in [json!(true), json!("1"), json!([]), json!({})] {
            let mut value: Value = serde_json::from_str(&raw).unwrap();
            value["entries"][0][field] = wrong;
            assert_eq!(parse(&value.to_string()).unwrap_err(), "PROGRESS_INVALID");
        }
    }
}

#[test]
fn row_limit_includes_tombstones_and_union() {
    let mut doc = rows(1, MAX_ROWS, "x");
    validate(&doc).unwrap();
    for row in &mut doc.entries {
        row.body.clear();
        row.deleted_at = Some(row.updated_at);
    }
    validate(&doc).unwrap();
    let left = Document {
        format_version: 1,
        entries: doc.entries[..MAX_ROWS / 2].to_vec(),
    };
    let right = Document {
        format_version: 1,
        entries: doc.entries[MAX_ROWS / 2..].to_vec(),
    };
    assert_eq!(merge(&left, &right).unwrap().entries.len(), MAX_ROWS);
    doc.entries.push(entry(MAX_ROWS + 1));
    assert_eq!(validate(&doc).unwrap_err(), "PROGRESS_LIMIT");
    assert_eq!(encode(&doc).unwrap_err(), "PROGRESS_LIMIT");
    assert_eq!(
        parse(&serde_json::to_string(&doc).unwrap()).unwrap_err(),
        "PROGRESS_LIMIT"
    );
    let additional = rows(MAX_ROWS + 1, 1, "x");
    assert_eq!(
        merge(&left, &merge(&right, &additional).unwrap()).unwrap_err(),
        "PROGRESS_LIMIT"
    );
}

#[test]
fn raw_utf8_byte_limit_is_inclusive() {
    let raw = encode(&rows(1, 1, "\u{4e2d}")).unwrap();
    let mut padded = String::with_capacity(MAX_BYTES + 1);
    padded.push_str(&raw);
    padded.extend(std::iter::repeat_n(' ', MAX_BYTES - raw.len()));
    assert_eq!(padded.len(), MAX_BYTES);
    assert!(padded.encode_utf16().count() < MAX_BYTES);
    parse(&padded).unwrap();
    padded.push(' ');
    assert_eq!(parse(&padded).unwrap_err(), "PROGRESS_LIMIT");
}

#[test]
fn canonical_byte_limit_and_merge_growth() {
    let mut doc = rows(1, 7000, &"x".repeat(1000));
    for row in &mut doc.entries {
        row.updated_at = 1_000_000_000_000_000;
    }
    let mut baseline = serde_json::to_string(&doc).unwrap().len();
    if (MAX_BYTES - baseline) % 2 == 1 {
        doc.entries.last_mut().unwrap().body.pop();
        baseline -= 1;
    }
    let mut replacements = (MAX_BYTES - baseline) / 2;
    for row in &mut doc.entries {
        let count = replacements.min(row.body.len());
        row.body = format!(
            "{}{}",
            "\u{4e2d}".repeat(count),
            "x".repeat(row.body.len() - count)
        );
        replacements -= count;
    }
    assert_eq!(replacements, 0);
    let encoded = encode(&doc).unwrap();
    assert_eq!(encoded.len(), MAX_BYTES);
    parse(&encoded).unwrap();
    drop(encoded);
    let left = Document {
        format_version: 1,
        entries: doc.entries[..3500].to_vec(),
    };
    let right = Document {
        format_version: 1,
        entries: doc.entries[3500..].to_vec(),
    };
    assert_eq!(merge(&left, &right).unwrap(), doc);
    // A JSON escape grows by one byte without changing the UTF-16 body length.
    let row = doc.entries.last_mut().unwrap();
    assert!(row.body.ends_with('x'));
    row.body.pop();
    row.body.push('"');
    assert_eq!(serde_json::to_string(&doc).unwrap().len(), MAX_BYTES + 1);
    assert_eq!(validate(&doc).unwrap_err(), "PROGRESS_LIMIT");
    assert_eq!(encode(&doc).unwrap_err(), "PROGRESS_LIMIT");
    let compact_raw = serde_json::to_string(&doc)
        .unwrap()
        .replace("\"updated_at\":1000000000000000", "\"updated_at\":1e15");
    assert!(compact_raw.len() < MAX_BYTES);
    assert_eq!(parse(&compact_raw).unwrap_err(), "PROGRESS_LIMIT");
    let oversized_right = Document {
        format_version: 1,
        entries: doc.entries[3500..].to_vec(),
    };
    assert_eq!(
        merge(&left, &oversized_right).unwrap_err(),
        "PROGRESS_LIMIT"
    );
    assert_eq!(
        filter_purged(&doc, &HashSet::new()).unwrap_err(),
        "PROGRESS_LIMIT"
    );
}

#[test]
fn object_key_uses_original_bytes_and_all_occupied_domains() {
    let base = "folder/todos.json";
    let key = object_key(base, &[]).unwrap();
    for other in [
        "Folder/todos.json",
        "folder/todos%2Ejson",
        "folder/e\u{301}.json",
        "folder/\u{e9}.json",
    ] {
        assert_ne!(key, object_key(other, &[]).unwrap());
    }
    assert_ne!(
        object_key("folder/e\u{301}.json", &[]).unwrap(),
        object_key("folder/\u{e9}.json", &[]).unwrap()
    );
    assert_eq!(
        object_key(base, &[base.into(), "notes.json".into(), key.clone()]).unwrap_err(),
        "PROGRESS_KEY_COLLISION"
    );
    assert_eq!(
        object_key(base, &[base.into(), "notes.json".into()]).unwrap(),
        key
    );
    assert!(object_key("task-note-links.json", &[]).is_ok());
    assert!(object_key(&"x".repeat(1024), &[]).is_ok());
    assert_eq!(
        object_key(&"x".repeat(1025), &[]).unwrap_err(),
        "PROGRESS_KEY_INVALID"
    );
    assert_eq!(
        object_key(&"\u{4e2d}".repeat(342), &[]).unwrap_err(),
        "PROGRESS_KEY_INVALID"
    );
}
