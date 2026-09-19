CREATE TABLE daily_plan_events(task_uuid TEXT NOT NULL,event_id TEXT NOT NULL,PRIMARY KEY(task_uuid,event_id));
CREATE TABLE daily_plans(task_uuid TEXT NOT NULL,plan_date TEXT NOT NULL,included INTEGER NOT NULL CHECK(included IN(0,1)),position INTEGER NOT NULL,clock INTEGER NOT NULL,writer TEXT NOT NULL,basis TEXT NOT NULL,PRIMARY KEY(task_uuid,plan_date));
CREATE TABLE daily_plan_completions(task_uuid TEXT NOT NULL,plan_date TEXT NOT NULL,event_id TEXT NOT NULL,basis TEXT NOT NULL,position INTEGER NOT NULL,PRIMARY KEY(task_uuid,plan_date,event_id));
CREATE TABLE daily_plan_operations(operation_uuid TEXT PRIMARY KEY,payload TEXT NOT NULL);
CREATE TABLE daily_plan_sync_state(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL DEFAULT 0 CHECK(revision BETWEEN 0 AND 9007199254740991),synced_revision INTEGER NOT NULL DEFAULT 0 CHECK(synced_revision BETWEEN 0 AND revision),etag TEXT,generation INTEGER NOT NULL DEFAULT 0 CHECK(generation BETWEEN 0 AND 9007199254740991));
INSERT INTO daily_plan_sync_state(id) VALUES(1);
CREATE VIEW daily_plan_basis AS SELECT task_uuid,group_concat(event_id,'|') AS basis FROM (SELECT task_uuid,event_id FROM daily_plan_events ORDER BY task_uuid,event_id) GROUP BY task_uuid;
CREATE TRIGGER daily_plans_insert AFTER INSERT ON daily_plans BEGIN UPDATE daily_plan_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER daily_plans_update AFTER UPDATE ON daily_plans WHEN NEW.included<>OLD.included OR NEW.position<>OLD.position OR NEW.clock<>OLD.clock OR NEW.writer<>OLD.writer OR NEW.basis<>OLD.basis BEGIN UPDATE daily_plan_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER daily_plans_delete AFTER DELETE ON daily_plans BEGIN UPDATE daily_plan_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER daily_plan_events_insert AFTER INSERT ON daily_plan_events BEGIN UPDATE daily_plan_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER daily_plan_completions_insert AFTER INSERT ON daily_plan_completions BEGIN UPDATE daily_plan_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER daily_plan_completions_delete AFTER DELETE ON daily_plan_completions BEGIN UPDATE daily_plan_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER daily_plan_completions_update AFTER UPDATE ON daily_plan_completions WHEN NEW.basis<>OLD.basis OR NEW.position<>OLD.position BEGIN UPDATE daily_plan_sync_state SET revision=revision+1 WHERE id=1; END;
CREATE TRIGGER daily_plan_task_lifecycle BEFORE UPDATE ON todos
WHEN (NEW.completed<>OLD.completed OR NEW.archived_at IS NOT OLD.archived_at OR NEW.deleted_at IS NOT OLD.deleted_at)
 AND NOT EXISTS(SELECT 1 FROM app_metadata WHERE key='daily.plan.remote-apply.v1' AND value='1')
BEGIN
  INSERT OR IGNORE INTO daily_plan_completions(task_uuid,plan_date,event_id,basis,position)
    SELECT OLD.uuid,p.plan_date,CAST(NEW.updated_at AS TEXT)||':'||lower(hex(NEW.updated_by))||':'||NEW.completed||':'||COALESCE(CAST(NEW.archived_at AS TEXT),'-')||':'||COALESCE(CAST(NEW.deleted_at AS TEXT),'-'),p.basis,p.position
    FROM daily_plans p WHERE p.task_uuid=OLD.uuid AND p.included=1
      AND p.basis=COALESCE((SELECT basis FROM daily_plan_basis WHERE task_uuid=OLD.uuid),'')
      AND OLD.completed=0 AND NEW.completed=1 AND OLD.archived_at IS NULL AND OLD.deleted_at IS NULL AND NEW.archived_at IS NULL AND NEW.deleted_at IS NULL;
  INSERT OR IGNORE INTO daily_plan_events(task_uuid,event_id) VALUES(NEW.uuid,CAST(NEW.updated_at AS TEXT)||':'||lower(hex(NEW.updated_by))||':'||NEW.completed||':'||COALESCE(CAST(NEW.archived_at AS TEXT),'-')||':'||COALESCE(CAST(NEW.deleted_at AS TEXT),'-'));
END;
CREATE TRIGGER daily_plan_task_removed AFTER DELETE ON todos BEGIN
  DELETE FROM daily_plans WHERE task_uuid=OLD.uuid;
  DELETE FROM daily_plan_completions WHERE task_uuid=OLD.uuid;
END;
