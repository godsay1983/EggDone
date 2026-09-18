//! Versioned space envelopes. Legacy keys retain their exact existing wire representation.
use serde::{Deserialize, Serialize};

pub const PREFIX: &str = "eggdone-spaces/v2/";
pub const FILES: [&str; 8] = [
    "todos.json",
    "notes.json",
    "note-attachments.json",
    "recurrence-rules.json",
    "task-note-links.json",
    "task-checklist-items.json",
    "task-checklist-definitions.json",
    "task-templates.json",
];
const DOMAINS: [&str; 8] = [
    "todos",
    "notes",
    "attachments",
    "rules",
    "links",
    "items",
    "definitions",
    "templates",
];
const MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    format_version: u32,
    space_uuid: String,
    domain: String,
    payload: String,
}
pub fn scope(key: &str) -> Result<Option<(&str, &str)>, String> {
    if !key.starts_with("eggdone-spaces/") {
        return Ok(None);
    }
    let rest = key.strip_prefix(PREFIX).ok_or("SYNC_SPACE_KEY")?;
    let (id, file) = rest.split_once('/').ok_or("SYNC_SPACE_KEY")?;
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "SYNC_SPACE_KEY")?;
    if parsed.to_string() != id {
        return Err("SYNC_SPACE_KEY".into());
    }
    let index = FILES
        .iter()
        .position(|name| *name == file)
        .ok_or("SYNC_SPACE_KEY")?;
    Ok(Some((id, DOMAINS[index])))
}
pub fn require_existing(key: &str, etag: Option<&str>) -> Result<(), String> {
    if scope(key)?.is_some() && etag.is_none() {
        return Err("SYNC_SPACE_INCOMPLETE".into());
    }
    Ok(())
}
pub fn require_probe(key: &str, status: u16) -> Result<(), String> {
    if scope(key)?.is_some() && status != 200 {
        return Err(if status == 404 {
            "SYNC_SPACE_INCOMPLETE"
        } else {
            "SYNC_SPACE_RESPONSE"
        }
        .into());
    }
    Ok(())
}
// Remove only when activation proof and terminal-aware production synchronization are wired together.
pub fn require_runtime_ready(key: &str) -> Result<(), String> {
    if scope(key)?.is_some() {
        return Err("SYNC_SPACE_ACTIVATION_REQUIRED".into());
    }
    Ok(())
}
pub fn decode(key: &str, raw: &str) -> Result<String, String> {
    let Some((id, domain)) = scope(key)? else {
        return Ok(raw.into());
    };
    if raw.len() > MAX_BYTES {
        return Err("SYNC_SPACE_LIMIT".into());
    }
    let e: Envelope = serde_json::from_str(raw).map_err(|_| "SYNC_SPACE_INVALID")?;
    if e.format_version != 2 || e.space_uuid != id || e.domain != domain {
        return Err("SYNC_SPACE_MISMATCH".into());
    }
    validate_payload(&e.payload)?;
    Ok(e.payload)
}
fn validate_payload(raw: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| "SYNC_SPACE_INVALID")?;
    if value.get("format_version").and_then(|v| v.as_u64()) != Some(1) {
        return Err("SYNC_SPACE_INVALID".into());
    }
    Ok(())
}
pub fn encode(key: &str, raw: &str) -> Result<String, String> {
    let Some((id, domain)) = scope(key)? else {
        return Ok(raw.into());
    };
    if raw.len() > MAX_BYTES {
        return Err("SYNC_SPACE_LIMIT".into());
    }
    validate_payload(raw)?;
    let encoded = serde_json::to_string(&Envelope {
        format_version: 2,
        space_uuid: id.into(),
        domain: domain.into(),
        payload: raw.into(),
    })
    .map_err(|_| "SYNC_SPACE_INVALID")?;
    let limit = match domain {
        "rules" => 3 * 1024 * 1024,
        "links" | "items" | "definitions" | "templates" => 4 * 1024 * 1024,
        _ => 5 * 1024 * 1024,
    };
    if encoded.len() > limit {
        return Err("SYNC_SPACE_LIMIT".into());
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_wire_and_invalid_envelopes() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../docs/fixtures/sync-space-v2.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let key = case["key"].as_str().unwrap();
            let raw = case["raw"].as_str().unwrap();
            if let Some(expected) = case["decoded"].as_str() {
                assert_eq!(decode(key, raw).unwrap(), expected, "{}", case["name"]);
                if scope(key).unwrap().is_some() {
                    assert_eq!(encode(key, expected).unwrap(), raw);
                }
            } else {
                assert!(decode(key, raw).is_err(), "{}", case["name"]);
            }
        }
    }
    #[test]
    fn no_implicit_creation_and_legacy_is_unchanged() {
        for file in FILES {
            let key = format!("{PREFIX}00000000-0000-4000-8000-000000000001/{file}");
            assert!(require_existing(&key, None).is_err());
            assert!(require_existing(&key, Some("\"etag\"")).is_ok());
            for status in [204, 403, 404, 500] {
                assert!(require_probe(&key, status).is_err());
            }
            assert!(require_probe(&key, 200).is_ok());
            let raw = "{\"format_version\":1}";
            let encoded = encode(&key, raw).unwrap();
            assert_eq!(decode(&key, &encoded).unwrap(), raw);
            assert!(decode(&key, raw).is_err());
        }
        assert_eq!(
            encode("account/todos.json", "legacy bytes").unwrap(),
            "legacy bytes"
        );
        assert!(require_existing("account/todos.json", None).is_ok());
        assert!(require_runtime_ready("account/todos.json").is_ok());
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&mut db).unwrap();
        db.execute(
            "UPDATE sync_settings SET enabled=1,object_key=?1",
            [format!(
                "{PREFIX}00000000-0000-4000-8000-000000000001/todos.json"
            )],
        )
        .unwrap();
        assert!(
            matches!(crate::s3_sync::prepare_manual_sync(&db), Err(e) if e == "SYNC_SPACE_ACTIVATION_REQUIRED")
        );
        assert!(encode(
            &format!("{PREFIX}00000000-0000-4000-8000-000000000001/todos.json"),
            "{\"format_version\":2}"
        )
        .is_err());
    }

    #[test]
    fn bounded_native_base_transport_and_strict_response() {
        use crate::recurrence_transport::tests::{Reply, Server};
        tauri::async_runtime::block_on(async {
            let key = format!("{PREFIX}00000000-0000-4000-8000-000000000001/todos.json");
            let raw = "{\"format_version\":1}";
            let encoded = encode(&key, raw).unwrap();
            let server = Server::new(vec![Reply::new(200, Some("\"one\""), encoded.as_bytes())]);
            let (text, etag) = crate::s3_sync::download_space_json(&server.bucket(), &key)
                .await
                .unwrap();
            assert_eq!(text, raw);
            assert_eq!(etag, "\"one\"");
            for status in [204, 206, 301, 403, 404, 500] {
                let server = Server::new(vec![Reply::new(status, Some("\"one\""), b"")]);
                assert!(crate::s3_sync::download_space_json(&server.bucket(), &key)
                    .await
                    .is_err());
                let server = Server::new(vec![Reply::new(status, Some("\"one\""), b"")]);
                let rules = crate::recurrence_transport::RecurrenceTransport::new(
                    &server.bucket(),
                    &key,
                    &[],
                )
                .unwrap();
                assert!(matches!(rules.download().await, Err(e) if e.starts_with("SYNC_SPACE_")));
            }
            for tag in [None, Some("W/\"one\""), Some("bad")] {
                let server = Server::new(vec![Reply::new(200, tag, encoded.as_bytes())]);
                assert!(crate::s3_sync::download_space_json(&server.bucket(), &key)
                    .await
                    .is_err());
            }
            let server =
                Server::new(vec![Reply::new(200, Some("\"one\""), encoded.as_bytes())
                    .with_header("ETag", "\"two\"")]);
            assert!(crate::s3_sync::download_space_json(&server.bucket(), &key)
                .await
                .is_err());
            let large = vec![b' '; 5 * 1024 * 1024 + 1];
            let server = Server::new(vec![Reply::new(200, Some("\"one\""), &large)]);
            assert!(crate::s3_sync::download_space_json(&server.bucket(), &key)
                .await
                .is_err());
        });
    }
}
