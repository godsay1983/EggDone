//! Opt-in tests against the disposable loopback server owned by run-s3-integration.ps1.
//! These public fixture credentials must never be used for a persistent service.
use super::*;
use s3::{creds::Credentials, region::Region, BucketConfiguration};

const ACCESS: &str = "eggdone-ns7-test-access";
const SECRET: &str = "eggdone-ns7-public-test-fixture";

fn target(secret: &str) -> (Box<Bucket>, String) {
    let run = std::env::var("EGGDONE_NS7_S3_RUN").expect("Run the isolated integration script");
    assert_eq!(run.len(), 32);
    assert!(run.bytes().all(|c| c.is_ascii_hexdigit()));
    let port = std::env::var("EGGDONE_NS7_S3_PORT")
        .unwrap()
        .parse::<u16>()
        .unwrap();
    assert!(port >= 1024);
    let bucket = Bucket::new(
        &format!("eggdone-ns7-{run}"),
        Region::Custom {
            region: "us-east-1".into(),
            endpoint: format!("http://127.0.0.1:{port}"),
        },
        Credentials::new(Some(ACCESS), Some(secret), None, None, None).unwrap(),
    )
    .unwrap()
    .with_path_style();
    (bucket, format!("{run}/同步/%2F/todos.json"))
}

fn document() -> RecurrenceDocument {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-document-v1.json"
    ))
    .unwrap();
    let mut rule = fixture["base_rule"].clone();
    rule["updated_by"] = "ns7-desktop".into();
    recurrence_protocol::parse_document(
        &serde_json::json!({ "format_version": 1, "rules": [rule] }).to_string(),
    )
    .unwrap()
}

#[test]
#[ignore = "Requires the disposable loopback S3 integration script; never a production bucket"]
fn ns7_s3_prepare() {
    tauri::async_runtime::block_on(async {
        let (bucket, key) = target(SECRET);
        let created = Bucket::create_with_path_style(
            &bucket.name,
            bucket.region.clone(),
            Credentials::new(Some(ACCESS), Some(SECRET), None, None, None).unwrap(),
            BucketConfiguration::default(),
        )
        .await
        .expect("Create the disposable bucket");
        assert_eq!(created.response_code, 200);
        let transport = RecurrenceTransport::new(&bucket, &key, &[]).unwrap();
        assert_eq!(transport.probe().await.unwrap(), "missing");
        let missing = transport.download().await.unwrap();
        assert!(missing.document.is_none());
        let rules = document();
        assert!(matches!(
            transport.upload(&rules, &missing).await.unwrap(),
            RuleUploadOutcome::Uploaded { .. }
        ));
        assert_eq!(
            transport.upload(&rules, &missing).await.unwrap(),
            RuleUploadOutcome::Conflict
        );
        assert_eq!(transport.download().await.unwrap().document, Some(rules));
        let (denied_bucket, _) = target("intentionally-invalid-fixture");
        let denied = RecurrenceTransport::new(&denied_bucket, &key, &[]).unwrap();
        assert_eq!(denied.probe().await.unwrap(), "denied");
        assert_eq!(
            denied.download().await.err().unwrap(),
            "RECURRENCE_DOWNLOAD_HTTP:403"
        );
        println!("NS7_S3_DESKTOP_PREPARE_OK");
    });
}

#[test]
#[ignore = "Requires Harmony to update the disposable S3 object first"]
fn ns7_s3_verify() {
    tauri::async_runtime::block_on(async {
        let (bucket, key) = target(SECRET);
        let transport = RecurrenceTransport::new(&bucket, &key, &[]).unwrap();
        let remote = transport.download().await.unwrap();
        let mut expected = document();
        expected.rules[0].updated_by = "ns7-harmony".into();
        expected.rules[0].updated_at = 2000;
        assert_eq!(remote.document.as_ref(), Some(&expected));
        let before = transport.probe().await.unwrap();
        expected.rules[0].updated_by = "ns7-desktop-final".into();
        expected.rules[0].updated_at = 3000;
        assert!(matches!(
            transport.upload(&expected, &remote).await.unwrap(),
            RuleUploadOutcome::Uploaded { .. }
        ));
        assert_ne!(transport.probe().await.unwrap(), before);
        assert_eq!(
            transport.upload(&document(), &remote).await.unwrap(),
            RuleUploadOutcome::Conflict
        );
        assert_eq!(transport.download().await.unwrap().document, Some(expected));
        println!("NS7_S3_DESKTOP_VERIFY_OK");
    });
}
