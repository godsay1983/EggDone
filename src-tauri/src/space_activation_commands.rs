use crate::{
    commands::lock_database,
    db::Database,
    migration_backup as backup,
    migration_backup_commands::migration_local_backup,
    s3_sync::{self, SyncRuntime},
    space_activation as space,
};
use tauri::{AppHandle, Manager};

fn active_report() -> space::Report {
    space::Report {
        missing: vec![],
        state: "active".into(),
        mode: String::new(),
        confirmation: None,
        object_key: None,
    }
}

#[tauri::command]
pub async fn migration_space(
    app: AppHandle,
    action: String,
    expected: Option<String>,
    accept_missing: Option<bool>,
) -> Result<space::Report, String> {
    if !["status", "prepare", "activate"].contains(&action.as_str()) {
        return Err("MIGRATION_SPACE_INVALID".into());
    }
    {
        let db = app.state::<Database>();
        if space::is_active(&*lock_database(&db)?)? {
            return Ok(active_report());
        }
        if action != "status" {
            let mut c = lock_database(&db)?;
            if !crate::task_workflow_protocol::is_empty(
                &crate::task_workflow_store::snapshot(&mut c)?.document,
            ) {
                return Err("MIGRATION_WORKFLOW_ACTIVE".into());
            }
            if !crate::daily_plan_protocol::is_empty(
                &crate::daily_plan_store::snapshot(&mut c)?.document,
            ) {
                return Err("MIGRATION_PLANNING_ACTIVE".into());
            }
        }
    }
    if action == "prepare" {
        migration_local_backup(app.clone(), "prepare".into(), None).await?;
        let handle = app.clone();
        let associated = tauri::async_runtime::spawn_blocking(move || {
            let db = handle.state::<Database>();
            let source = s3_sync::prepare_migration_asset_source(&*lock_database(&db)?)?;
            let mut target = s3_sync::MigrationSpaceTarget::new(&source, None)?;
            space::association(&mut target, source.main_key())
        })
        .await
        .map_err(|_| "MIGRATION_SPACE_WORKER")??;
        if associated.is_none() {
            migration_local_backup(app.clone(), "prepareCloud".into(), None).await?;
            migration_local_backup(app.clone(), "preparePublication".into(), None).await?;
        }
        let db = app.state::<Database>();
        let c = lock_database(&db)?;
        let local = backup::latest(&c)?.ok_or("MIGRATION_BACKUP_CHANGED")?;
        backup::require_current(&c, &local)?;
        let (claim, mode) = match associated {
            Some(claim) => (claim, "join"),
            None => (
                space::Claim::from_plan(
                    &backup::publication::latest(&c)?.ok_or("MIGRATION_PUBLICATION_CHANGED")?,
                )?,
                "create",
            ),
        };
        if s3_sync::migration_source_binding(&c)? != claim.plan()?.source {
            return Err("MIGRATION_SPACE_CHANGED".into());
        }
        let pending = space::Pending {
            claim,
            local,
            mode: mode.into(),
        };
        space::save_pending(&c, &pending)?;
        return pending.report();
    }
    let pending = {
        let db = app.state::<Database>();
        let c = lock_database(&db)?;
        let pending = space::pending(&c)?;
        if action == "status" {
            return match pending {
                Some(p) if backup::require_current(&c, &p.local).is_ok() => p.report(),
                _ => Ok(space::Report {
                    missing: vec![],
                    state: "idle".into(),
                    mode: String::new(),
                    confirmation: None,
                    object_key: None,
                }),
            };
        }
        let p = pending.ok_or("MIGRATION_SPACE_CONFIRMATION")?;
        p.require_confirmation(expected.as_deref(), accept_missing == Some(true))?;
        backup::require_current(&c, &p.local)?;
        p
    };
    let handle = app.clone();
    let claim = pending.claim.clone();
    let associated = tauri::async_runtime::spawn_blocking(move || {
        let db = handle.state::<Database>();
        let source = s3_sync::prepare_migration_asset_source(&*lock_database(&db)?)?;
        let mut target = s3_sync::MigrationSpaceTarget::new(&source, Some(claim.clone()))?;
        space::association(&mut target, &claim.1)
    })
    .await
    .map_err(|_| "MIGRATION_SPACE_WORKER")??;
    if associated
        .as_ref()
        .is_some_and(|claim| claim != &pending.claim)
    {
        return Err("MIGRATION_SPACE_CHANGED".into());
    }
    if pending.mode == "create" && associated.is_none() {
        let digest = {
            let db = app.state::<Database>();
            let c = lock_database(&db)?;
            let publication =
                backup::publication::latest(&c)?.ok_or("MIGRATION_PUBLICATION_CHANGED")?;
            if space::Claim::from_plan(&publication)? != pending.claim {
                return Err("MIGRATION_SPACE_CHANGED".into());
            }
            backup::publication::plan_digest(&publication)?
        };
        migration_local_backup(app.clone(), "publish".into(), Some(digest)).await?;
    }
    tauri::async_runtime::spawn_blocking(move || {
        let db = app.state::<Database>();
        let runtime = app.state::<SyncRuntime>();
        let _guard = runtime.acquire().map_err(|_| "MIGRATION_SYNC_BUSY")?;
        let root = app
            .path()
            .app_data_dir()
            .map_err(|_| "MIGRATION_BACKUP_IO")?
            .join("migration-backups");
        backup::verify_files(&root, &pending.local)?;
        backup::rehearse(&root, &pending.local)?;
        let guard = || -> Result<(), String> {
            let c = lock_database(&db)?;
            backup::require_current(&c, &pending.local)?;
            if s3_sync::migration_source_binding(&c)? != pending.claim.plan()?.source {
                return Err("MIGRATION_SPACE_CHANGED".into());
            }
            Ok(())
        };
        guard()?;
        let source = s3_sync::prepare_migration_asset_source(&*lock_database(&db)?)?;
        let mut target = s3_sync::MigrationSpaceTarget::new(&source, Some(pending.claim.clone()))?;
        let ledger = space::initialize(&mut target, &pending.claim, &guard)?;
        let assets = crate::note_attachment_sync::build_backup_document(
            &*lock_database(&db)?,
            crate::db::now_millis(),
        )?;
        let prefix = pending
            .claim
            .main()?
            .trim_end_matches("todos.json")
            .to_string();
        for asset in &assets.attachments {
            if ledger
                .terminals
                .iter()
                .any(|t| t.kind == "note" && t.uuid == asset.note_uuid)
            {
                continue;
            }
            for entry in backup::asset_files(&pending.local)
                .iter()
                .filter(|f| f.name.starts_with(&format!("{}-", asset.uuid)))
                .filter(|f| !f.missing)
            {
                guard()?;
                let bytes = backup::read_asset(&root, &pending.local, entry)?;
                let suffix = &entry.name[37..];
                let mime = if suffix == "preview.jpg" {
                    "image/jpeg"
                } else {
                    &asset.mime_type
                };
                space::create_exact(
                    &mut target,
                    &format!("{prefix}note-assets/v1/{}/{suffix}", asset.uuid),
                    &bytes,
                    mime,
                )?;
            }
        }
        guard()?;
        let ledger = space::verify_current(&mut target, &pending.claim)?;
        space::activate(&mut *lock_database(&db)?, &pending, &ledger)?;
        Ok(active_report())
    })
    .await
    .map_err(|_| "MIGRATION_SPACE_WORKER".to_string())?
}
