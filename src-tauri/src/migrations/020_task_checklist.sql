CREATE TABLE IF NOT EXISTS task_checklist_items (
 uuid TEXT PRIMARY KEY, todo_uuid TEXT NOT NULL, active INTEGER NOT NULL CHECK(active IN (0,1)), record_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_checklist_items_todo ON task_checklist_items(todo_uuid,active);

CREATE TABLE IF NOT EXISTS task_checklist_definitions (
 rule_uuid TEXT PRIMARY KEY, active INTEGER NOT NULL CHECK(active IN (0,1)), record_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_checklist_sync_state (
 domain TEXT PRIMARY KEY CHECK(domain IN ('items','definitions')),
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0 AND revision<=9007199254740991),
 synced_revision INTEGER NOT NULL DEFAULT 0 CHECK(synced_revision>=0 AND synced_revision<=revision),
 etag TEXT,
 generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0 AND generation<=9007199254740991)
);

INSERT OR IGNORE INTO task_checklist_sync_state(domain) VALUES ('items'),('definitions');

CREATE TABLE IF NOT EXISTS task_checklist_operations (
 operation_uuid TEXT PRIMARY KEY, payload TEXT NOT NULL, result_updated_at INTEGER NOT NULL
 CHECK(result_updated_at>=0 AND result_updated_at<=9007199254740991)
);

CREATE TRIGGER IF NOT EXISTS task_checklist_items_insert AFTER INSERT ON task_checklist_items

BEGIN UPDATE task_checklist_sync_state SET revision=revision+1 WHERE domain='items'; END;

CREATE TRIGGER IF NOT EXISTS task_checklist_items_update AFTER UPDATE ON task_checklist_items
WHEN NEW.record_json<>OLD.record_json
BEGIN UPDATE task_checklist_sync_state SET revision=revision+1 WHERE domain='items'; END;

CREATE TRIGGER IF NOT EXISTS task_checklist_items_delete AFTER DELETE ON task_checklist_items

BEGIN UPDATE task_checklist_sync_state SET revision=revision+1 WHERE domain='items'; END;

CREATE TRIGGER IF NOT EXISTS task_checklist_definitions_insert AFTER INSERT ON task_checklist_definitions

BEGIN UPDATE task_checklist_sync_state SET revision=revision+1 WHERE domain='definitions'; END;

CREATE TRIGGER IF NOT EXISTS task_checklist_definitions_update AFTER UPDATE ON task_checklist_definitions
WHEN NEW.record_json<>OLD.record_json
BEGIN UPDATE task_checklist_sync_state SET revision=revision+1 WHERE domain='definitions'; END;

CREATE TRIGGER IF NOT EXISTS task_checklist_definitions_delete AFTER DELETE ON task_checklist_definitions

BEGIN UPDATE task_checklist_sync_state SET revision=revision+1 WHERE domain='definitions'; END;
