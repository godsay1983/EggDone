'use strict';

// LC0b executable specification only; not imported by either application.
const { isDeepStrictEqual } = require('node:util');
const MAX = Number.MAX_SAFE_INTEGER;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
function fail(code) { throw new Error(code); }
function id(value) {
  if (typeof value !== 'string' || !UUID.test(value)) fail('INVALID_ID');
}
function stamp(value) {
  if (!Number.isSafeInteger(value) || value < 0) fail('INVALID_VERSION');
}
function date(value) {
  if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(value) ||
      value < '0001-01-01' || value > '9999-12-31') fail('INVALID_DATE');
  const parsed = new Date(value + 'T00:00:00.000Z');
  if (!Number.isFinite(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== value) fail('INVALID_DATE');
  return value;
}
function barrierSet(values) {
  if (!Array.isArray(values)) fail('INVALID_BARRIERS');
  values.forEach(id);
  if (new Set(values).size !== values.length) fail('INVALID_BARRIERS');
  return values.slice().sort();
}
function mergeBarriers(left, right) {
  return [...new Set([...barrierSet(left), ...barrierSet(right)])].sort();
}
function matchesBasis(basis, current) {
  return isDeepStrictEqual(barrierSet(basis), barrierSet(current));
}
function lifecycle(parent, terminal = false) {
  if (terminal) return 'purged';
  if (parent === null) return 'missing';
  if (parent.deleted_at !== null) return 'deleted';
  if (parent.archived_at !== null) return 'archived';
  if (parent.completed) return 'completed';
  return 'active';
}
function childValid(child, parent, barriers) {
  if (child === null) return false;
  id(child.todo_uuid);
  // Basis never installs its own evidence into the authoritative barrier set.
  return child.todo_uuid === parent.uuid && matchesBasis(child.basis, barriers);
}
function project({ parent, terminal = false, barriers, plan, workflow, today }) {
  date(today);
  barrierSet(barriers);
  const state = lifecycle(parent, terminal);
  if (state !== 'active') return { state, planned: false, actionable: false, review_due: false };
  const planned = childValid(plan, parent, barriers) && plan.removed === false && date(plan.plan_date) === today;
  const validWorkflow = childValid(workflow, parent, barriers);
  if (validWorkflow && !['ready', 'waiting'].includes(workflow.state)) fail('INVALID_WORKFLOW');
  const waiting = validWorkflow && workflow.state === 'waiting';
  const review = waiting && workflow.review_date !== null ? date(workflow.review_date) : null;
  return { state: waiting ? 'waiting' : 'ready', planned,
    actionable: planned && !waiting, review_due: review !== null && review <= today };
}
function validateWaiting(reason, review) {
  if (typeof reason !== 'string' || reason.length > 200) fail('INVALID_REASON');
  if (review !== null) date(review);
  return { reason, review_date: review };
}
function archiveAction(current, expected, action, context) {
  if (!['unarchive', 'reopen', 'delete'].includes(action)) fail('ARCHIVE_INVALID_ACTION');
  id(expected.uuid);
  if (context.terminal) fail('ARCHIVE_TERMINAL');
  if (current === null) fail('ARCHIVE_NOT_FOUND');
  if (!isDeepStrictEqual(current, expected)) fail('ARCHIVE_CONFLICT');
  if (lifecycle(current) !== 'archived') fail('ARCHIVE_NOT_ARCHIVED');
  if (current.current_rule_active) fail('ARCHIVE_RULE_ACTIVE');
  stamp(context.now);
  stamp(current.updated_at);
  if (typeof context.by !== 'string' || !UUID.test(context.by)) fail('ARCHIVE_INVALID_VERSION');
  const next = Math.max(context.now, current.updated_at + 1);
  if (!Number.isSafeInteger(next) || next > MAX) fail('ARCHIVE_INVALID_VERSION');
  const task = structuredClone(current);
  task.updated_at = next;
  task.updated_by = context.by;
  // Never call ordinary complete/delete recurrence advancement for a historical archive.
  task.reminder_at = null;
  if (action === 'delete') task.deleted_at = next;
  else {
    task.archived_at = null;
    if (!task.group_valid) task.group_uuid = null;
    if (action === 'reopen') {
      task.completed = false;
      task.completed_at = null;
      task.repeat_rule = null;
      task.repeat_series_uuid = null;
      task.repeat_next_due_date = null;
    }
  }
  return { task, end_arrangements: true, register_reminder: false, advance_recurrence: false,
    tombstone_links: action === 'delete' };
}
function freezeTargets(rows) {
  if (!Array.isArray(rows) || rows.length === 0) fail('ARCHIVE_EMPTY_SELECTION');
  const seen = new Set();
  for (const row of rows) {
    id(row.uuid);
    if (seen.has(row.uuid)) fail('ARCHIVE_DUPLICATE_TARGET');
    seen.add(row.uuid);
  }
  return structuredClone(rows);
}
module.exports = { archiveAction, barrierSet, date, freezeTargets, lifecycle,
  matchesBasis, mergeBarriers, project, validateWaiting };
