DROP TRIGGER daily_plan_task_lifecycle;
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
