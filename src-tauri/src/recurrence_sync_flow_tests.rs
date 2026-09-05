use super::*;
use crate::{
    recurrence_links::LinkPlan, recurrence_snapshot, recurrence_store, sync_runtime_state,
};
use rusqlite::Connection;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
struct Case {
    id: String,
    todo: Vec<String>,
    rules: Vec<String>,
    expected: String,
    attempts: usize,
    #[serde(default)]
    blocked: usize,
    failure: Option<String>,
    switch_after: Option<String>,
}

struct Port {
    db: Connection,
    case: Case,
    events: Vec<String>,
    downloads: usize,
    todo_puts: usize,
    rule_puts: usize,
    target_current: bool,
    mutate: Option<String>,
    payloads: Vec<String>,
}

impl Port {
    fn new(case: Case) -> Self {
        Self {
            db: recurrence_snapshot::tests::setup(true, false, false),
            case,
            events: vec![],
            downloads: 0,
            todo_puts: 0,
            rule_puts: 0,
            target_current: true,
            mutate: None,
            payloads: vec![],
        }
    }
    fn event(&mut self, event: &str) -> Result<(), String> {
        self.events.push(event.into());
        if self.case.failure.as_deref() == Some(event) {
            return Err("INJECTED_FAILURE".into());
        }
        if self.case.switch_after.as_deref() == Some(event) {
            self.target_current = false;
        }
        if self.mutate.as_deref() == Some(event) {
            self.db
                .execute(
                    "UPDATE todos SET title='edited during upload',updated_at=30000",
                    [],
                )
                .unwrap();
            let mut rules = recurrence_store::snapshot(&self.db).unwrap().document;
            rules.rules[0].updated_at = 30000;
            recurrence_store::merge(&mut self.db, &rules).unwrap();
        }
        Ok(())
    }
    fn dirty(&self) -> bool {
        self.db
            .query_row("SELECT dirty_domains FROM sync_runtime_state", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
            .contains("todos")
    }
}

impl RecurrenceSyncPort for Port {
    type Remote = usize;
    async fn target_is_current(&mut self) -> Result<bool, String> {
        Ok(self.target_current)
    }
    async fn download(&mut self) -> Result<usize, String> {
        self.downloads += 1;
        self.event("download")?;
        Ok(self.downloads)
    }
    async fn prepare(&mut self, remote: &usize) -> Result<SnapshotPreparation, String> {
        assert_eq!(*remote, self.downloads);
        self.event("prepare")?;
        if self.downloads <= self.case.blocked {
            return Ok(SnapshotPreparation {
                snapshot: None,
                links: LinkPlan {
                    links: vec![],
                    can_prepare: false,
                },
            });
        }
        // A later full retry must include changes discovered since the earlier download.
        if self.downloads > 1 {
            let mut document = crate::sync::build_document(&self.db, 9000).unwrap();
            for todo in &mut document.todos {
                todo.title = "remote retry update".into();
                todo.updated_at = 20000;
            }
            crate::sync::merge_remote_document(&mut self.db, &document, 9000).unwrap();
        }
        recurrence_snapshot::prepare_snapshot(&mut self.db, 10000 + self.downloads as i64)
    }
    async fn upload_todos(
        &mut self,
        remote: &usize,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> Result<bool, String> {
        assert_eq!(*remote, self.downloads);
        self.event("put-todos")?;
        self.payloads.push(snapshot.todo_json.clone());
        let result = self.case.todo[self.todo_puts] == "success";
        self.todo_puts += 1;
        Ok(result)
    }
    async fn acknowledge_todos(&mut self, revision: i64) -> Result<bool, String> {
        self.event("ack-todos")?;
        let tx = self.db.transaction().unwrap();
        let result = sync_runtime_state::mark_domain_synced(
            &tx,
            sync_runtime_state::SyncDomain::Todos,
            revision,
        )?;
        tx.commit().unwrap();
        Ok(result)
    }
    async fn upload_rules(
        &mut self,
        remote: &usize,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> Result<RuleUploadOutcome, String> {
        assert_eq!(*remote, self.downloads);
        self.event("put-rules")?;
        let todo: serde_json::Value = serde_json::from_str(self.payloads.last().unwrap()).unwrap();
        let rules = crate::recurrence_protocol::parse_document(&snapshot.rules_json).unwrap();
        for rule in rules
            .rules
            .iter()
            .filter(|r| !r.exhausted && r.deleted_at.is_none())
        {
            assert!(todo["todos"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["uuid"] == rule.current_todo_uuid));
        }
        let uploaded = self.case.rules[self.rule_puts] == "uploaded";
        self.rule_puts += 1;
        Ok(if uploaded {
            RuleUploadOutcome::Uploaded {
                etag: format!("\"put-{}\"", self.downloads),
            }
        } else {
            RuleUploadOutcome::Conflict
        })
    }
    async fn acknowledge_rules(&mut self, revision: i64, etag: &str) -> Result<bool, String> {
        self.event("ack-rules")?;
        assert_eq!(etag, format!("\"put-{}\"", self.downloads));
        recurrence_store::acknowledge(&self.db, revision, etag)
    }
    async fn snapshot_is_current(
        &mut self,
        snapshot: &RecurrenceUploadSnapshot,
    ) -> Result<bool, String> {
        self.event("check-revisions")?;
        let todo: i64 = self
            .db
            .query_row(
                "SELECT todos_dirty_version FROM sync_runtime_state",
                [],
                |r| r.get(0),
            )
            .unwrap();
        Ok(todo == snapshot.todo_revision
            && recurrence_store::snapshot(&self.db)?.revision == snapshot.rule_revision)
    }
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!(
        "../../docs/fixtures/recurrence-sync-flow-v1.json"
    ))
    .unwrap()
}

#[test]
fn shared_ordered_flow_cases_use_real_snapshot_and_cas() {
    tauri::async_runtime::block_on(async {
        for case in cases() {
            let mut port = Port::new(case.clone());
            let result = run(&mut port).await;
            if case.expected == "success" {
                let result = result.unwrap();
                assert_eq!(result.attempts, case.attempts, "{}", case.id);
                assert!(!result.local_changes_pending);
                assert!(!port.dirty());
                let state = recurrence_store::snapshot(&port.db).unwrap();
                assert_eq!(state.revision, state.synced_revision);
                if port.payloads.len() > 1 {
                    assert!(port
                        .payloads
                        .last()
                        .unwrap()
                        .contains("remote retry update"));
                }
            } else {
                assert_eq!(result.unwrap_err(), case.expected, "{}", case.id);
                if !port.events.iter().any(|e| e == "ack-todos") {
                    assert!(port.dirty());
                }
                if !port.events.iter().any(|e| e == "ack-rules") {
                    let state = recurrence_store::snapshot(&port.db).unwrap();
                    assert!(state.revision > state.synced_revision);
                }
            }
            assert_eq!(port.downloads, case.attempts, "{}", case.id);
            assert_eq!(port.todo_puts, case.todo.len(), "{}", case.id);
            assert_eq!(port.rule_puts, case.rules.len(), "{}", case.id);
            // Each rule PUT must belong to a round that already uploaded and acknowledged Todo.
            for (i, event) in port.events.iter().enumerate() {
                if event == "put-rules" {
                    assert_eq!(port.events[i - 1], "ack-todos");
                }
                if event == "ack-rules" {
                    assert_eq!(port.events[i - 1], "put-rules");
                }
            }
            if case.id == "rule-conflict" {
                assert_eq!(port.events.iter().filter(|e| *e == "prepare").count(), 2);
            }
        }
    });
}

#[test]
fn edits_during_upload_are_not_acknowledged_by_old_snapshots() {
    tauri::async_runtime::block_on(async {
        for event in ["put-todos", "put-rules", "ack-rules"] {
            let mut port = Port::new(cases().remove(0));
            port.mutate = Some(event.into());
            let result = run(&mut port).await.unwrap();
            assert!(result.local_changes_pending);
            assert!(!result.rules_acknowledged);
            let rules = recurrence_store::snapshot(&port.db).unwrap();
            assert!(rules.revision > rules.synced_revision);
            assert!(port.dirty());
            assert!(!port.payloads[0].contains("edited during upload"));
        }
    });
}
