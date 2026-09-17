'use strict';
// Candidate rules only: no database, app, device, network or user's data.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const api = require('./task-lifecycle-reference.cjs');
const root = path.resolve(__dirname, '..');
const f = JSON.parse(fs.readFileSync(path.join(root, 'docs/fixtures/task-lifecycle-v1.json'), 'utf8'));
const clone = value => structuredClone(value);
let count = 0;
function test(name, action) {
  try { action(); count++; } catch (cause) { throw new Error(name, { cause }); }
}
const ids = new Set();
for (const c of [...f.archive_cases, ...f.projection_cases]) {
  assert(!ids.has(c.id), 'duplicate fixture ID: ' + c.id);
  ids.add(c.id);
}
assert.equal(f.contract_version, 1);
for (const c of f.archive_cases) test(c.id, () => {
  const expected = { ...clone(f.base_archive), ...clone(c.patch) };
  const current = c.missing ? null : { ...clone(expected), ...clone(c.concurrent ?? {}) };
  const context = { now: c.now ?? 200, by: c.by ?? '123e4567-e89b-42d3-a456-426614174005',
    terminal: c.terminal ?? false };
  const before = clone({ expected, current, context });
  const run = () => api.archiveAction(current, expected, c.action, context);
  if (c.error) assert.throws(run, { message: c.error });
  else {
    const actual = run();
    assert.deepEqual(actual, { task: { ...expected, ...c.expectedPatch },
      end_arrangements: true, register_reminder: false, advance_recurrence: false,
      tombstone_links: c.action === 'delete' });
    assert.deepEqual(run(), actual, 'pure result must be deterministic');
  }
  assert.deepEqual({ expected, current, context }, before, 'no input mutation on success or failure');
});
for (const c of f.projection_cases) test(c.id, () => {
  const input = { ...clone(f.base_projection), ...clone(c.patch) };
  const before = clone(input);
  assert.deepEqual(api.project(input), c.expected);
  assert.deepEqual(input, before);
});
for (const value of f.valid_dates) test('valid date ' + value, () => assert.equal(api.date(value), value));
for (const value of f.invalid_dates) test('invalid date ' + value, () =>
  assert.throws(() => api.date(value), { message: 'INVALID_DATE' }));
test('waiting UTF-16 boundary and optional date', () => {
  const reason = '\ud83e\udd5a'.repeat(100);
  assert.deepEqual(api.validateWaiting(reason, null), { reason, review_date: null });
  assert.throws(() => api.validateWaiting(reason + 'x', null), /INVALID_REASON/);
  assert.deepEqual(api.validateWaiting('', '2026-09-17'), { reason: '', review_date: '2026-09-17' });
  assert.throws(() => api.validateWaiting(null, null), /INVALID_REASON/);
  assert.throws(() => api.validateWaiting('', '2026-02-29'), /INVALID_DATE/);
});
test('boundary merge is associative, commutative and idempotent', () => {
  const [a, b] = f.barrier_ids;
  const sets = [[], [a], [b], [a, b], [b, a]];
  for (const x of sets) {
    assert.deepEqual(api.mergeBarriers(x, x), api.barrierSet(x));
    for (const y of sets) {
      assert.deepEqual(api.mergeBarriers(x, y), api.mergeBarriers(y, x));
      for (const z of sets) assert.deepEqual(
        api.mergeBarriers(api.mergeBarriers(x, y), z),
        api.mergeBarriers(x, api.mergeBarriers(y, z)));
    }
  }
});
test('boundary input order is immaterial; duplicates and bad IDs fail', () => {
  const [a, b] = f.barrier_ids;
  assert(api.matchesBasis([b, a], [a, b]));
  assert(!api.matchesBasis([], [a]));
  assert(!api.matchesBasis([a], []));
  assert.throws(() => api.barrierSet([a, a]), /INVALID_BARRIERS/);
  assert.throws(() => api.barrierSet(['bad']), /INVALID_ID/);
});
test('edit does not expire a plan; completion and undo never revive it', () => {
  const input = clone(f.base_projection);
  input.parent.title = 'New title';
  input.parent.updated_at = Number.MAX_SAFE_INTEGER;
  assert.equal(api.project(input).actionable, true);
  input.barriers = [f.barrier_ids[0]];
  input.parent.completed = true;
  assert.equal(api.project(input).actionable, false);
  input.parent.completed = false;
  assert.equal(api.project(input).planned, false);
  input.plan.basis = input.barriers.slice();
  assert.equal(api.project(input).actionable, true);
});
test('unknown child evidence never mutates known barriers', () => {
  const input = clone(f.base_projection);
  input.plan.basis = f.barrier_ids.slice();
  assert.equal(api.project(input).planned, false);
  assert.deepEqual(input.barriers, []);
});
test('fixed selection is detached from UI and excludes later deletions', () => {
  const rows = [clone(f.base_archive)];
  const frozen = api.freezeTargets(rows);
  rows[0].title = 'Later edit';
  rows.push({ ...clone(f.base_archive), uuid: f.barrier_ids[0] });
  assert.deepEqual(frozen, [f.base_archive]);
  assert.throws(() => api.freezeTargets([]), /EMPTY_SELECTION/);
  assert.throws(() => api.freezeTargets([f.base_archive, f.base_archive]), /DUPLICATE_TARGET/);
});
test('retry without receipt is a conflict, never a second archive write', () => {
  const old = clone(f.base_archive);
  const result = api.archiveAction(old, old, 'reopen',
    { now: 200, by: '123e4567-e89b-42d3-a456-426614174005' });
  assert.throws(() => api.archiveAction(result.task, old, 'reopen',
    { now: 300, by: '123e4567-e89b-42d3-a456-426614174005' }), /ARCHIVE_CONFLICT/);
});
const peerArg = process.argv.find(arg => arg.startsWith('--peer='));
if (peerArg) test('mirrored artifacts match byte-for-byte', () => {
  for (const file of ['docs/TASK_LIFECYCLE_CONTRACT.md', 'docs/fixtures/task-lifecycle-v1.json',
    'scripts/task-lifecycle-reference.cjs', 'scripts/test-task-lifecycle-contract.cjs',
    'docs/TASK_LIFECYCLE_IMPLEMENTATION_PLAN.md', 'docs/TASK_LIFECYCLE_ROADMAP.md']) {
    assert.deepEqual(fs.readFileSync(path.join(root, file)),
      fs.readFileSync(path.join(peerArg.slice(7), file)), file);
  }
});
console.log('PASS: ' + count + ' candidate-contract checks; 125 barrier association combinations. No production/native/sync acceptance claimed.');
