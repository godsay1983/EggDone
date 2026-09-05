CREATE TABLE IF NOT EXISTS recurrence_rules (
  uuid TEXT PRIMARY KEY,
  current_todo_uuid TEXT NOT NULL,
  active INTEGER NOT NULL CHECK(active IN (0,1)),
  record_json TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_recurrence_active_todo
  ON recurrence_rules(current_todo_uuid) WHERE active = 1;
CREATE TABLE IF NOT EXISTS recurrence_sync_state (
  id INTEGER PRIMARY KEY CHECK(id = 1),
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0 AND revision <= 9007199254740991),
  synced_revision INTEGER NOT NULL DEFAULT 0 CHECK(synced_revision >= 0 AND synced_revision <= revision),
  etag TEXT
);
INSERT OR IGNORE INTO recurrence_sync_state(id) VALUES (1);
CREATE TRIGGER IF NOT EXISTS recurrence_dirty_insert AFTER INSERT ON recurrence_rules
BEGIN UPDATE recurrence_sync_state SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS recurrence_dirty_update AFTER UPDATE ON recurrence_rules
WHEN NEW.record_json <> OLD.record_json
BEGIN UPDATE recurrence_sync_state SET revision = revision + 1 WHERE id = 1; END;
CREATE TRIGGER IF NOT EXISTS recurrence_dirty_delete AFTER DELETE ON recurrence_rules
BEGIN UPDATE recurrence_sync_state SET revision = revision + 1 WHERE id = 1; END;
