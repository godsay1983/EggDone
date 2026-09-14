// P1a reference-contract tests; no production database, network, device or calendar engine is exercised.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const cp = require('node:child_process');
const api = require('./task-checklist-reference.cjs');
const root = path.resolve(__dirname, '..');
const fixturePath = path.join(root, 'docs/fixtures/task-checklist-v1.json');
const f = JSON.parse(fs.readFileSync(fixturePath, 'utf8'));
const clone = v => JSON.parse(JSON.stringify(v));
const normalize = text => text.replace(/\r\n/g, '\n').trimEnd();
const generated = cp.execFileSync(process.execPath, [path.join(__dirname, 'generate-task-checklist-fixtures.cjs')], { encoding: 'utf8', maxBuffer: 2 * 1024 * 1024 });
assert.equal(normalize(generated), normalize(fs.readFileSync(fixturePath, 'utf8')), 'fixture generator drift');
const peerArg = process.argv.find(a => a.startsWith('--peer='));
if (peerArg) {
  const peer = peerArg.slice('--peer='.length);
  for (const file of ['docs/TASK_CHECKLIST_PROTOCOL.md', 'docs/fixtures/task-checklist-v1.json',
    'scripts/task-checklist-reference.cjs', 'scripts/generate-task-checklist-fixtures.cjs',
    'scripts/test-task-checklist-contract.cjs', 'docs/TASK_PRODUCTIVITY_ROADMAP.md']) {
    assert.equal(normalize(fs.readFileSync(path.join(root, file), 'utf8')),
      normalize(fs.readFileSync(path.join(peer, file), 'utf8')), file + ' differs');
  }
}
let count = 0;
function test(name, action) {
  try { action(); count++; } catch (error) { throw new Error(name, { cause: error }); }
}
for (const c of f.identities) test('identity', () => assert.equal(api.itemUuid(c.todo, c.entry), c.expected));
test('UUID DNS reference vector', () => assert.equal(api.uuidV5('www.widgets.com'), '21f7f8de-8051-5b89-8680-0195ef798b6a'));
test('unchanged recurrence parent identity', () => assert.equal(api.uuidV5(f.recurrence_parent.key), f.recurrence_parent.expected));
for (const c of f.valid) test(c.id, () => {
  const value = api.parse(JSON.stringify(c.document), c.kind);
  assert.deepEqual(value, c.document);
  const encoded = api.encode(value, c.kind);
  assert.deepEqual(api.parse(encoded, c.kind), JSON.parse(encoded));
  assert.equal(api.encode(api.parse(encoded, c.kind), c.kind), encoded);
});
for (const c of f.invalid) test(c.id, () => assert.throws(() => api.parse(JSON.stringify(c.document), c.kind)));
for (const c of f.merges) test(c.id, () => {
  const before = clone(c);
  if (c.error) {
    assert.throws(() => api.merge(c.left, c.right, c.kind), new RegExp(c.error));
    assert.throws(() => api.merge(c.right, c.left, c.kind), new RegExp(c.error));
  } else {
    assert.deepEqual(api.merge(c.left, c.right, c.kind), c.expected);
    assert.deepEqual(api.merge(c.right, c.left, c.kind), c.expected);
    assert.deepEqual(api.merge(c.expected, c.left, c.kind), c.expected);
  }
  assert.deepEqual(c, before, 'merge must not mutate inputs');
});
for (const c of f.generation) test(c.id, () => {
  const before = clone(c);
  if (c.error) {
    assert.throws(() => api.materialize(c.definition, c.existing, c.context), new RegExp(c.error));
  } else {
    const result = api.materialize(c.definition, c.existing, c.context);
    assert.deepEqual(result, { state: c.state, document: c.expected });
    assert.deepEqual(api.materialize(c.definition, result.document, c.context), result, 'backfill idempotence');
  }
  assert.deepEqual(c, before, 'backfill must not mutate inputs');
});
for (const c of f.backups) test(c.id, () => {
  if (c.error) assert.throws(() => api.backupExtensions(c.input));
  else assert.deepEqual(api.backupExtensions(c.input), c.expected);
});
for (const c of f.keys) test('key: ' + c.todo, () => {
  if (c.error) assert.throws(() => api.objectKey(c.todo, c.kind, c.occupied));
  else assert.equal(api.objectKey(c.todo, c.kind, c.occupied), c.expected);
});
test('strict identities and JSON structure', () => {
  assert.throws(() => api.itemUuid(f.identities[0].todo.toUpperCase(), f.identities[0].entry));
  assert.throws(() => api.itemUuid('bad', f.identities[0].entry));
  assert.throws(() => api.parse('{', 'items'));
  assert.throws(() => api.parse('[]', 'definitions'));
  const json = JSON.stringify({ format_version: 1, items: [f.base_item] })
    .replace('"format_version":1', '"format_version":1e0').replace('"updated_at":2000', '"updated_at":2e3');
  assert.equal(api.parse(json, 'items').items[0].updated_at, 2000);
});
test('local cap never truncates valid remote records', () => {
  const remote = Array.from({ length: 21 }, (_, i) => ({ ...f.base_item,
    uuid: '123e4567-e89b-42d3-a456-' + (i + 500).toString(16).padStart(12, '0') }));
  const doc = { format_version: 1, items: remote };
  assert.equal(api.merge(doc, { format_version: 1, items: [] }, 'items').items.length, 21);
});
test('document safety limits', () => {
  assert.throws(() => api.checkDocument({ format_version: 1, items: Array(10001).fill(f.base_item) }, 'items'));
  assert.throws(() => api.checkDocument({ format_version: 1, definitions: Array(2001).fill(f.base_definition) }, 'definitions'));
  assert.throws(() => api.validateDefinition({ ...f.base_definition, entries: Array(1001).fill(f.base_definition.entries[0]) }));
  assert.throws(() => api.parse(' '.repeat(4 * 1024 * 1024) + '{}', 'items'));
  const text = JSON.stringify({ format_version: 1, items: Array.from({ length: 5000 }, (_, i) => ({
    ...f.base_item, content: '字'.repeat(200),
    uuid: '123e4567-e89b-42d3-a456-' + (i + 500).toString(16).padStart(12, '0')
  })) });
  assert.ok(text.length < 4 * 1024 * 1024 && Buffer.byteLength(text) > 4 * 1024 * 1024);
  assert.throws(() => api.parse(text, 'items'), 'limit is UTF-8 bytes, not UTF-16 length');
});
test('definition snapshot identity protects history', () => {
  const left = { format_version: 1, definitions: [f.base_definition] };
  for (const change of [
    { schedule: { ...f.base_definition.schedule, interval: 2 } },
    { first_todo_uuid: f.base_item.uuid },
    { created_at: 999 }
  ]) {
    assert.throws(() => api.merge(left, { format_version: 1,
      definitions: [{ ...f.base_definition, ...change }] }, 'definitions'), /DEFINITION_IDENTITY_CONFLICT/);
  }
});
test('definition tombstone prevents new seed rows', () => {
  const c = f.generation[0];
  const result = api.materialize({ ...c.definition, updated_at: 2000, deleted_at: 2000 }, c.existing, c.context);
  assert.deepEqual(result, { state: 'suppressed', document: c.existing });
});
test('backup merge cannot revive a deleted child with a future clock', () => {
  const local = { format_version: 1, items: [{ ...f.base_item, deleted_at: 2100, updated_at: 2100 }] };
  const incoming = { format_version: 1, items: [{ ...f.base_item, updated_at: Number.MAX_SAFE_INTEGER }] };
  assert.deepEqual(api.merge(local, incoming, 'items'), local);
});
test('single-item conflict merge is associative for every bounded combination', () => {
  const variants = [
    f.base_item, { ...f.base_item, completed: true }, { ...f.base_item, content: '🥚' },
    { ...f.base_item, content: '\ue000' }, { ...f.base_item, updated_by: 'device-z' },
    { ...f.base_item, updated_at: 1000, deleted_at: 1000 },
    { ...f.base_item, updated_at: 3000, completed: true }
  ].map(item => ({ format_version: 1, items: [item] }));
  for (const a of variants) for (const b of variants) for (const c of variants) {
    assert.deepEqual(api.merge(api.merge(a, b, 'items'), c, 'items'),
      api.merge(a, api.merge(b, c, 'items'), 'items'));
  }
});
test('union limits fail without discarding either input', () => {
  const rows = Array.from({ length: 10002 }, (_, i) => ({ ...f.base_item,
    uuid: '123e4567-e89b-42d3-a456-' + (i + 20000).toString(16).padStart(12, '0') }));
  const a = { format_version: 1, items: rows.slice(0, 5001) };
  const b = { format_version: 1, items: rows.slice(5001) };
  api.checkDocument(a, 'items'); api.checkDocument(b, 'items');
  assert.throws(() => api.merge(a, b, 'items'));
  assert.equal(a.items.length, 5001); assert.equal(b.items.length, 5001);
});
test('definition aggregate limit is independent of per-definition limit', () => {
  const entries = Array.from({ length: 1000 }, (_, i) => ({
    uuid: '123e4567-e89b-42d3-a456-' + (i + 30000).toString(16).padStart(12, '0'), content: 'x', sort_order: i
  }));
  const definitions = Array.from({ length: 21 }, (_, i) => ({
    ...f.base_definition, rule_uuid: '123e4567-e89b-42d3-a456-' + (i + 40000).toString(16).padStart(12, '0'), entries
  }));
  assert.equal(api.checkDocument({ format_version: 1, definitions: definitions.slice(0, 20) }, 'definitions').definitions.length, 20);
  assert.throws(() => api.checkDocument({ format_version: 1, definitions }, 'definitions'));
});
test('object keys reject invalid Unicode and UTF-8 overflow', () => {
  assert.throws(() => api.objectKey('bad' + String.fromCharCode(0xd800) + '/todos.json', 'items'));
  assert.throws(() => api.objectKey('字'.repeat(340) + '/todos.json', 'items'));
});
console.log('PASS: ' + count + ' reference-contract cases, including 343 associative combinations' +
  (peerArg ? '; peer artifacts match' : '') + '. Production adapters and native acceptance remain pending.');
