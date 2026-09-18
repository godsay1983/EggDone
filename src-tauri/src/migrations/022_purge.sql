CREATE TABLE IF NOT EXISTS lifecycle_terminals (
 kind TEXT NOT NULL CHECK(kind IN ('todo','note')), uuid TEXT NOT NULL,
 operation_uuid TEXT NOT NULL, purged_at INTEGER NOT NULL CHECK(purged_at>=0 AND purged_at<=9007199254740991),
 PRIMARY KEY(kind,uuid)
);

CREATE TABLE IF NOT EXISTS lifecycle_sync_state (
 id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0 AND revision<=9007199254740991),
 synced_revision INTEGER NOT NULL DEFAULT 0 CHECK(synced_revision>=0 AND synced_revision<=revision),
 etag TEXT
);

INSERT OR IGNORE INTO lifecycle_sync_state(id) VALUES(1);

CREATE TABLE IF NOT EXISTS purge_plans (
 operation_uuid TEXT PRIMARY KEY, target_epoch TEXT NOT NULL, created_at INTEGER NOT NULL,
 total INTEGER NOT NULL, attachments INTEGER NOT NULL, bytes INTEGER NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('prepared','running','complete'))
);

CREATE TABLE IF NOT EXISTS purge_targets (
 operation_uuid TEXT NOT NULL REFERENCES purge_plans(operation_uuid) ON DELETE CASCADE,
 kind TEXT NOT NULL CHECK(kind IN ('todo','note')), uuid TEXT NOT NULL, fingerprint TEXT NOT NULL,
 outcome TEXT NOT NULL DEFAULT 'pending' CHECK(outcome IN ('pending','purged','already_purged','skipped_conflict','skipped_missing','skipped_rule')),
 PRIMARY KEY(operation_uuid,kind,uuid)
);

CREATE TABLE IF NOT EXISTS purge_cleanup (
 attachment_uuid TEXT PRIMARY KEY, note_uuid TEXT NOT NULL, original_path TEXT, preview_path TEXT,
 local_done INTEGER NOT NULL DEFAULT 0 CHECK(local_done IN (0,1)),
 local_attempts INTEGER NOT NULL DEFAULT 0,
 remote_done INTEGER NOT NULL DEFAULT 0 CHECK(remote_done IN (0,1))
);

CREATE TRIGGER IF NOT EXISTS lifecycle_terminal_insert AFTER INSERT ON lifecycle_terminals
BEGIN UPDATE lifecycle_sync_state SET revision=revision+1 WHERE id=1; END;

CREATE TRIGGER IF NOT EXISTS lifecycle_terminal_update AFTER UPDATE ON lifecycle_terminals
WHEN NEW.operation_uuid<>OLD.operation_uuid OR NEW.purged_at<>OLD.purged_at
BEGIN UPDATE lifecycle_sync_state SET revision=revision+1 WHERE id=1; END;

CREATE TRIGGER IF NOT EXISTS lifecycle_terminal_no_delete BEFORE DELETE ON lifecycle_terminals
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL_IMMUTABLE'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_terminal_identity BEFORE UPDATE ON lifecycle_terminals
WHEN NEW.kind<>OLD.kind OR NEW.uuid<>OLD.uuid
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL_IMMUTABLE'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_todos_insert BEFORE INSERT ON todos
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=NEW.uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_todos_update BEFORE UPDATE ON todos
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=NEW.uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_notes_insert BEFORE INSERT ON notes
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='note' AND uuid=NEW.uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_notes_update BEFORE UPDATE ON notes
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='note' AND uuid=NEW.uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_note_attachments_insert BEFORE INSERT ON note_attachments
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='note' AND uuid=NEW.note_uuid) OR EXISTS(SELECT 1 FROM purge_cleanup WHERE attachment_uuid=NEW.uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_note_attachments_update BEFORE UPDATE ON note_attachments
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='note' AND uuid=NEW.note_uuid) OR EXISTS(SELECT 1 FROM purge_cleanup WHERE attachment_uuid=NEW.uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_task_checklist_items_insert BEFORE INSERT ON task_checklist_items
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=NEW.todo_uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_task_checklist_items_update BEFORE UPDATE ON task_checklist_items
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=NEW.todo_uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_task_note_links_insert BEFORE INSERT ON task_note_links
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE (kind='todo' AND uuid=NEW.todo_uuid) OR (kind='note' AND uuid=NEW.note_uuid))
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_task_note_links_update BEFORE UPDATE ON task_note_links
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE (kind='todo' AND uuid=NEW.todo_uuid) OR (kind='note' AND uuid=NEW.note_uuid))
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_note_history_insert BEFORE INSERT ON note_history
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='note' AND uuid=NEW.note_uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;

CREATE TRIGGER IF NOT EXISTS lifecycle_guard_note_history_update BEFORE UPDATE ON note_history
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='note' AND uuid=NEW.note_uuid)
BEGIN SELECT RAISE(ABORT,'PURGE_TERMINAL'); END;
