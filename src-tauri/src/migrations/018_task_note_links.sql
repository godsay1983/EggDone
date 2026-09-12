CREATE TABLE IF NOT EXISTS task_note_links (
  uuid TEXT PRIMARY KEY,
  todo_uuid TEXT NOT NULL,
  note_uuid TEXT NOT NULL,
  active INTEGER NOT NULL CHECK(active IN (0,1)),
  record_json TEXT NOT NULL,
  UNIQUE(todo_uuid,note_uuid)
);
CREATE INDEX IF NOT EXISTS idx_task_note_links_todo ON task_note_links(todo_uuid,active);
CREATE INDEX IF NOT EXISTS idx_task_note_links_note ON task_note_links(note_uuid,active);
CREATE TABLE IF NOT EXISTS task_note_link_sync_state (
  id INTEGER PRIMARY KEY CHECK(id=1),
  revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0 AND revision <= 9007199254740991),
  synced_revision INTEGER NOT NULL DEFAULT 0 CHECK(synced_revision >= 0 AND synced_revision <= revision),
  etag TEXT
);
INSERT OR IGNORE INTO task_note_link_sync_state(id) VALUES (1);
CREATE TRIGGER IF NOT EXISTS task_note_link_dirty_insert AFTER INSERT ON task_note_links
BEGIN UPDATE task_note_link_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS task_note_link_dirty_update AFTER UPDATE ON task_note_links
WHEN NEW.record_json <> OLD.record_json
BEGIN UPDATE task_note_link_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER IF NOT EXISTS task_note_link_dirty_delete AFTER DELETE ON task_note_links
BEGIN UPDATE task_note_link_sync_state SET revision=revision+1 WHERE id=1; END;
