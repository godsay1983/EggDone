CREATE TABLE IF NOT EXISTS note_history (
  id INTEGER PRIMARY KEY AUTOINCREMENT CHECK(id <= 9007199254740991),
  note_uuid TEXT NOT NULL,
  title TEXT NOT NULL,
  content TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  updated_by TEXT NOT NULL,
  captured_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_note_history_note ON note_history(note_uuid,id DESC);

CREATE TRIGGER IF NOT EXISTS notes_capture_history BEFORE UPDATE OF title,content ON notes
WHEN (OLD.title <> NEW.title OR OLD.content <> NEW.content)
  AND (length(trim(OLD.title)) > 0 OR length(trim(OLD.content)) > 0)
BEGIN
  INSERT INTO note_history(note_uuid,title,content,updated_at,updated_by,captured_at)
  SELECT OLD.uuid,OLD.title,OLD.content,OLD.updated_at,OLD.updated_by,
    CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
  WHERE NOT EXISTS (
    SELECT 1 FROM note_history WHERE id=(SELECT MAX(id) FROM note_history WHERE note_uuid=OLD.uuid)
    AND title=OLD.title AND content=OLD.content
  );
END;

CREATE TRIGGER IF NOT EXISTS note_history_retention AFTER INSERT ON note_history
BEGIN
  DELETE FROM note_history WHERE id IN (
    SELECT id FROM note_history WHERE note_uuid=NEW.note_uuid ORDER BY id DESC LIMIT -1 OFFSET 100
  );
  DELETE FROM note_history WHERE id IN (SELECT id FROM note_history ORDER BY id DESC LIMIT -1 OFFSET 1000);
END;

CREATE TRIGGER IF NOT EXISTS notes_delete_history AFTER DELETE ON notes
BEGIN DELETE FROM note_history WHERE note_uuid=OLD.uuid; END;
