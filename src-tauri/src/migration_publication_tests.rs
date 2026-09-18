use super::*;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

struct Memory {
    objects: BTreeMap<String, Vec<u8>>,
    calls: usize,
    fail: Option<usize>,
    after: bool,
    writes: usize,
}
impl Memory {
    fn event(&mut self, after: bool) -> Result<(), String> {
        if !after {
            self.calls += 1;
        }
        if self.fail == Some(self.calls) && after == self.after {
            return Err("MIGRATION_PUBLICATION_NETWORK".into());
        }
        Ok(())
    }
}
impl Storage for Memory {
    fn read(&mut self, key: &str, limit: usize) -> Result<Option<Vec<u8>>, String> {
        self.event(false)?;
        let b = self.objects.get(key).cloned();
        assert!(b.as_ref().is_none_or(|b| b.len() <= limit));
        self.event(true)?;
        Ok(b)
    }
    fn create(&mut self, key: &str, bytes: &[u8]) -> Result<(), String> {
        self.event(false)?;
        self.writes += 1;
        self.objects
            .entry(key.into())
            .or_insert_with(|| bytes.to_vec());
        self.event(true)
    }
}
fn fixture() -> (PathBuf, Connection, BackupPlan, cloud::Plan, Plan) {
    let root = std::env::temp_dir().join(format!("eggdone-publication-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let mut c = Connection::open(root.join("db")).unwrap();
    crate::db::migrate(&mut c).unwrap();
    c.execute_batch("UPDATE sync_settings SET endpoint='https://fixture.invalid',region='us-east-1',bucket='fixture',object_key='account/todos.json';
      INSERT INTO app_metadata(key,value) VALUES('sync.target.epoch.v1','settled');
      UPDATE sync_runtime_state SET last_result='success',last_success_at=1,dirty_domains='[]';
      UPDATE recurrence_sync_state SET synced_revision=revision;
      UPDATE task_note_link_sync_state SET synced_revision=revision;
      UPDATE task_checklist_sync_state SET synced_revision=revision;
      UPDATE task_template_sync_state SET synced_revision=revision;").unwrap();
    let work = super::super::prepare(&mut c, 1).unwrap();
    let copies = root.join("copies");
    super::super::copy(&work, &copies, &root.join("assets")).unwrap();
    super::super::finish(&mut c, &work.plan, 2).unwrap();
    let local = super::super::latest(&c).unwrap().unwrap();
    let mut remote: Vec<_> = (0..8)
        .map(|_| cloud::RemoteObject {
            etag: None,
            bytes: None,
        })
        .collect();
    remote[0] = cloud::RemoteObject {
        etag: Some("\"one\"".into()),
        bytes: Some(serde_json::to_vec(&crate::sync::build_document(&c, 1).unwrap()).unwrap()),
    };
    remote[3] = cloud::RemoteObject {
        etag: Some("\"rules\"".into()),
        bytes: Some(b"{\"format_version\":1,\"rules\":[]}".to_vec()),
    };
    let binding = crate::s3_sync::migration_source_binding(&c).unwrap();
    let cloud = cloud::prepare(&mut c, &local, &binding, "account/todos.json", &remote, 3).unwrap();
    cloud::copy(&copies, &local, &cloud, &remote, |_| panic!()).unwrap();
    cloud::finish(&mut c, &local, &cloud, 4).unwrap();
    let cloud = cloud::latest(&c).unwrap().unwrap();
    let p = prepare(&mut c, &local, &cloud).unwrap();
    (root, c, local, cloud, p)
}
fn run(
    c: &RefCell<Connection>,
    root: &Path,
    local: &BackupPlan,
    cloud: &cloud::Plan,
    p: &Plan,
    store: &mut Memory,
) -> Result<(), String> {
    publish(
        p,
        store,
        |f| cloud::read_file(&root.join("copies"), local, cloud, f),
        || require_current(&c.borrow(), local, cloud, p),
        || Ok(()),
        |done, published| {
            record(
                &mut c.borrow_mut(),
                local,
                cloud,
                p,
                &plan_digest(p)?,
                done,
                published,
            )
        },
    )
}
#[test]
fn publication_confirmation_reopen_and_all_transfer_failure_boundaries() {
    for after in [false, true] {
        for fail in 1..=11 {
            let (root, mut connection, local, cloud, p) = fixture();
            assert!(record(&mut connection, &local, &cloud, &p, "stale", None, false).is_err());
            assert!(!latest(&connection).unwrap().unwrap().confirmed);
            record(
                &mut connection,
                &local,
                &cloud,
                &p,
                &plan_digest(&p).unwrap(),
                None,
                false,
            )
            .unwrap();
            let p = latest(&connection).unwrap().unwrap();
            let c = RefCell::new(connection);
            let mut storage = Memory {
                objects: BTreeMap::new(),
                calls: 0,
                fail: Some(fail),
                after,
                writes: 0,
            };
            let result = run(&c, &root, &local, &cloud, &p, &mut storage);
            if fail <= storage.calls {
                assert!(result.is_err());
            }
            drop(c);
            let c = RefCell::new(Connection::open(root.join("db")).unwrap());
            let resumed = latest(&c.borrow()).unwrap().unwrap();
            assert_eq!(resumed.operation, p.operation);
            storage.fail = None;
            run(&c, &root, &local, &cloud, &resumed, &mut storage).unwrap();
            let complete = latest(&c.borrow()).unwrap().unwrap();
            assert!(complete.published);
            assert_eq!(complete.completed.len(), 2);
            let writes = storage.writes;
            run(&c, &root, &local, &cloud, &complete, &mut storage).unwrap();
            assert_eq!(storage.writes, writes);
            storage
                .objects
                .remove(&object_key(&p, &p.files[0]).unwrap());
            assert!(run(&c, &root, &local, &cloud, &complete, &mut storage)
                .unwrap_err()
                .contains("DAMAGED"));
            assert_eq!(storage.writes, writes);
            drop(c);
            fs::remove_dir_all(root).unwrap();
        }
    }
}
#[test]
fn publication_conflicts_changed_sources_and_unconfirmed_plans_never_overwrite() {
    let (root, mut c, local, cloud, p) = fixture();
    let mut storage = Memory {
        objects: BTreeMap::new(),
        calls: 0,
        fail: None,
        after: false,
        writes: 0,
    };
    assert!(publish(
        &p,
        &mut storage,
        |_| panic!(),
        || Ok(()),
        || Ok(()),
        |_, _| Ok(())
    )
    .is_err());
    assert_eq!(storage.calls, 0);
    record(
        &mut c,
        &local,
        &cloud,
        &p,
        &plan_digest(&p).unwrap(),
        None,
        false,
    )
    .unwrap();
    let p = latest(&c).unwrap().unwrap();
    let c = RefCell::new(c);
    let key = marker_key(&p).unwrap();
    storage.objects.insert(key.clone(), b"different".to_vec());
    assert_eq!(
        run(&c, &root, &local, &cloud, &p, &mut storage).unwrap_err(),
        "MIGRATION_PUBLICATION_CONFLICT"
    );
    assert_eq!(storage.writes, 0);
    storage.objects.clear();
    let calls = Cell::new(0);
    assert!(publish(
        &p,
        &mut storage,
        |f| cloud::read_file(&root.join("copies"), &local, &cloud, f),
        || Ok(()),
        || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err("MIGRATION_CLOUD_CHANGED".into())
            } else {
                Ok(())
            }
        },
        |_, _| Ok(())
    )
    .is_err());
    assert!(!storage.objects.contains_key(&key));
    c.borrow()
        .execute("UPDATE sync_settings SET bucket='changed'", [])
        .unwrap();
    assert!(run(&c, &root, &local, &cloud, &p, &mut storage).is_err());
    let mut invalid = p.clone();
    invalid.files[0].name = "../escape".into();
    assert!(validate(&invalid).is_err());
    invalid = p.clone();
    invalid.completed = vec!["unknown".into()];
    assert!(validate(&invalid).is_err());
    drop(c);
    fs::remove_dir_all(root).unwrap();
}
