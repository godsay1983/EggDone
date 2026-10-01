CREATE TABLE task_progress_entries (
 uuid TEXT PRIMARY KEY, task_uuid TEXT NOT NULL, body TEXT NOT NULL,
 created_at INTEGER NOT NULL CHECK(created_at BETWEEN 0 AND 9007199254740991),
 created_by TEXT NOT NULL, updated_at INTEGER NOT NULL CHECK(updated_at BETWEEN created_at AND 9007199254740991),
 updated_by TEXT NOT NULL, clock INTEGER NOT NULL CHECK(clock BETWEEN 1 AND 9007199254740991),
 deleted_at INTEGER, CHECK(deleted_at IS NULL OR (body='' AND deleted_at=updated_at))
);
CREATE INDEX task_progress_page ON task_progress_entries(task_uuid,created_at DESC,uuid DESC) WHERE deleted_at IS NULL;
CREATE INDEX task_progress_parent ON task_progress_entries(task_uuid);
CREATE TABLE task_progress_operations (
 operation_uuid TEXT PRIMARY KEY, request_digest TEXT NOT NULL,
 task_uuid TEXT NOT NULL, record_uuid TEXT NOT NULL, committed_token TEXT NOT NULL,
 write_revision INTEGER NOT NULL CHECK(write_revision BETWEEN 1 AND 9007199254740991),
 published INTEGER NOT NULL DEFAULT 0 CHECK(published IN(0,1))
);
CREATE INDEX task_progress_pending ON task_progress_operations(record_uuid,committed_token) WHERE published=0;
CREATE INDEX task_progress_operation_parent ON task_progress_operations(task_uuid);
CREATE TABLE task_progress_notices(task_uuid TEXT PRIMARY KEY);
CREATE TABLE task_progress_sync_state (
 id INTEGER PRIMARY KEY CHECK(id=1),
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision BETWEEN 0 AND 9007199254740991),
 synced_revision INTEGER NOT NULL DEFAULT 0 CHECK(synced_revision BETWEEN 0 AND revision),
 etag TEXT, generation INTEGER NOT NULL DEFAULT 0 CHECK(generation BETWEEN 0 AND 9007199254740991)
);
INSERT INTO task_progress_sync_state(id) VALUES(1);
CREATE TRIGGER task_progress_revision_guard BEFORE UPDATE ON task_progress_sync_state
WHEN NEW.revision>9007199254740991 OR NEW.generation>9007199254740991
BEGIN SELECT RAISE(ABORT,'PROGRESS_LIMIT'); END;
CREATE TRIGGER task_progress_insert AFTER INSERT ON task_progress_entries
BEGIN UPDATE task_progress_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER task_progress_update AFTER UPDATE ON task_progress_entries
WHEN NEW.body<>OLD.body OR NEW.updated_at<>OLD.updated_at OR NEW.updated_by<>OLD.updated_by OR NEW.clock<>OLD.clock OR NEW.deleted_at IS NOT OLD.deleted_at
BEGIN UPDATE task_progress_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER task_progress_delete AFTER DELETE ON task_progress_entries
BEGIN UPDATE task_progress_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER task_progress_identity BEFORE UPDATE ON task_progress_entries
WHEN NEW.uuid<>OLD.uuid OR NEW.task_uuid<>OLD.task_uuid OR NEW.created_at<>OLD.created_at OR NEW.created_by<>OLD.created_by
BEGIN SELECT RAISE(ABORT,'PROGRESS_CONFLICT'); END;
CREATE TRIGGER task_progress_no_restore BEFORE UPDATE ON task_progress_entries
WHEN OLD.deleted_at IS NOT NULL AND NEW.deleted_at IS NULL
BEGIN SELECT RAISE(ABORT,'PROGRESS_DELETED'); END;
CREATE TRIGGER task_progress_guard_insert BEFORE INSERT ON task_progress_entries
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=NEW.task_uuid)
BEGIN SELECT RAISE(ABORT,'PROGRESS_UNAVAILABLE'); END;
CREATE TRIGGER task_progress_guard_update BEFORE UPDATE ON task_progress_entries
WHEN EXISTS(SELECT 1 FROM lifecycle_terminals WHERE kind='todo' AND uuid=NEW.task_uuid)
BEGIN SELECT RAISE(ABORT,'PROGRESS_UNAVAILABLE'); END;
CREATE TRIGGER task_progress_parent_removed AFTER DELETE ON todos
BEGIN
 DELETE FROM task_progress_entries WHERE task_uuid=OLD.uuid;
 DELETE FROM task_progress_operations WHERE task_uuid=OLD.uuid;
 DELETE FROM task_progress_notices WHERE task_uuid=OLD.uuid;
END;
CREATE TRIGGER task_progress_terminal AFTER INSERT ON lifecycle_terminals WHEN NEW.kind='todo'
BEGIN
 UPDATE task_progress_sync_state SET revision=revision+1 WHERE id=1;
 DELETE FROM task_progress_entries WHERE task_uuid=NEW.uuid;
 DELETE FROM task_progress_operations WHERE task_uuid=NEW.uuid;
 DELETE FROM task_progress_notices WHERE task_uuid=NEW.uuid;
END;
