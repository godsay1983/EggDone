CREATE TABLE IF NOT EXISTS task_templates (
 uuid TEXT PRIMARY KEY, active INTEGER NOT NULL CHECK(active IN (0,1)), record_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_template_sync_state (
 id INTEGER PRIMARY KEY CHECK(id=1),
 revision INTEGER NOT NULL DEFAULT 0 CHECK(revision>=0 AND revision<=9007199254740991),
 synced_revision INTEGER NOT NULL DEFAULT 0 CHECK(synced_revision>=0 AND synced_revision<=revision),
 etag TEXT,
 generation INTEGER NOT NULL DEFAULT 0 CHECK(generation>=0 AND generation<=9007199254740991)
);

INSERT OR IGNORE INTO task_template_sync_state(id) VALUES(1);

CREATE TABLE IF NOT EXISTS task_template_operations (
 operation_uuid TEXT PRIMARY KEY, payload TEXT NOT NULL, result_json TEXT NOT NULL
);

CREATE TRIGGER IF NOT EXISTS task_templates_insert AFTER INSERT ON task_templates
BEGIN UPDATE task_template_sync_state SET revision=revision+1 WHERE id=1; END;

CREATE TRIGGER IF NOT EXISTS task_templates_update AFTER UPDATE ON task_templates
WHEN NEW.record_json<>OLD.record_json
BEGIN UPDATE task_template_sync_state SET revision=revision+1 WHERE id=1; END;

CREATE TRIGGER IF NOT EXISTS task_templates_delete AFTER DELETE ON task_templates
BEGIN UPDATE task_template_sync_state SET revision=revision+1 WHERE id=1; END;
