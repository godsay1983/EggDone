// Independent fixture generator: expected winners are explicit, not computed by the reference merger.
const { createHash } = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const clone = v => JSON.parse(JSON.stringify(v));
const id = n => '123e4567-e89b-42d3-a456-' + n.toString(16).padStart(12, '0');
function v5(name) {
  const b = createHash('sha1').update(Buffer.from('6ba7b8109dad11d180b400c04fd430c8', 'hex')).update(name).digest().subarray(0, 16);
  b[6] = (b[6] & 15) | 80; b[8] = (b[8] & 63) | 128;
  const s = b.toString('hex'); return s.slice(0, 8) + '-' + s.slice(8, 12) + '-' + s.slice(12, 16) + '-' + s.slice(16, 20) + '-' + s.slice(20);
}
const childId = (todo, entry) => v5('eggdone:task-checklist-item:v1:' + todo + ':' + entry);
const recurring = JSON.parse(fs.readFileSync(path.join(root, 'docs/fixtures/recurrence-v1.json'), 'utf8'))[0];
const rule = '123e4567-e89b-42d3-a456-426614174000';
const todo = id(2);
const item = { uuid: id(10), todo_uuid: todo, source_rule_uuid: null, source_entry_uuid: null,
  content: 'Compile', sort_order: 1000, completed: false,
  created_at: 1000, updated_at: 2000, updated_by: 'device-a', deleted_at: null };
const definition = { rule_uuid: rule, first_todo_uuid: todo, schedule: recurring.schedule,
  timezone_id: null, applies_from_index: 2,
  entries: [{ uuid: id(20), content: 'Compile', sort_order: 1000 }, { uuid: id(21), content: 'Upload', sort_order: 2000 }],
  created_at: 1000, updated_at: 1000, updated_by: 'device-a', deleted_at: null };
const doc = (kind, ...rows) => ({ format_version: 1, [kind]: rows });
const items = (...rows) => doc('items', ...rows), definitions = (...rows) => doc('definitions', ...rows);
const generated = definition.entries.map(entry => ({
  uuid: childId(recurring.uuid, entry.uuid), todo_uuid: recurring.uuid, source_rule_uuid: rule,
  source_entry_uuid: entry.uuid, content: entry.content, sort_order: entry.sort_order, completed: false,
  created_at: 1000, updated_at: 1000, updated_by: 'checklist-seed-v1', deleted_at: null
})).sort((a, b) => a.uuid < b.uuid ? -1 : 1);
const valid = [
  { id: 'empty-items', kind: 'items', document: items() },
  { id: 'manual', kind: 'items', document: items(item) },
  { id: 'inherited', kind: 'items', document: items(...generated) },
  { id: 'max-clock', kind: 'items', document: items({ ...item, updated_at: Number.MAX_SAFE_INTEGER }) },
  { id: 'unicode', kind: 'items', document: items({ ...item, content: '测试🥚' }) },
  { id: 'empty-definitions', kind: 'definitions', document: definitions() },
  { id: 'definition', kind: 'definitions', document: definitions(definition) },
  { id: 'explicit-empty-definition', kind: 'definitions', document: definitions({ ...definition, entries: [] }) },
  { id: 'reordered-entries', kind: 'definitions', document: definitions({ ...definition, entries: [...definition.entries].reverse() }) },
  { id: 'timed-definition', kind: 'definitions', document: definitions({ ...definition, schedule: { ...definition.schedule, local_time_minutes: 900 }, timezone_id: 'Asia/Shanghai' }) }
];
const invalid = [];
for (const [field, value] of [
  ['uuid', 'bad'], ['uuid', item.uuid.toUpperCase()], ['todo_uuid', todo + '\n'],
  ['source_rule_uuid', rule], ['source_entry_uuid', id(20)], ['content', ''], ['content', ' padded '],
  ['content', 'x'.repeat(201)], ['content', 'bad\nline'], ['content', 'bad\u202e'],
  ['content', 'bad\ud800'], ['content', 'bad\udc00'], ['content', null],
  ['sort_order', -1], ['sort_order', 1.5], ['sort_order', '1'], ['sort_order', Number.MAX_SAFE_INTEGER + 1],
  ['completed', 1], ['created_at', -1], ['created_at', 3000], ['updated_at', 0.5],
  ['updated_by', ''], ['updated_by', 'bad\n'], ['updated_by', 'x'.repeat(129)],
  ['deleted_at', 999], ['deleted_at', 2001], ['deleted_at', false], ['extra', 1]
]) invalid.push({ id: 'item-field-' + field + '-' + invalid.length, kind: 'items', document: items({ ...item, [field]: value }) });
for (const field of Object.keys(item)) {
  const missing = clone(item); delete missing[field];
  invalid.push({ id: 'missing-' + field, kind: 'items', document: items(missing) });
}
invalid.push(
  { id: 'duplicate-items', kind: 'items', document: items(item, item) },
  { id: 'wrong-derived-uuid', kind: 'items', document: items({ ...generated[0], uuid: id(999) }) },
  { id: 'wrong-version', kind: 'items', document: { format_version: 2, items: [] } },
  { id: 'extra-envelope', kind: 'items', document: { ...items(), extra: [] } },
  { id: 'null-row', kind: 'items', document: items(null) },
  { id: 'null-document', kind: 'items', document: null }
);
for (const [field, value] of [
  ['applies_from_index', 1], ['rule_uuid', 'bad'], ['first_todo_uuid', 'bad'],
  ['entries', null], ['entries', [definition.entries[0], definition.entries[0]]],
  ['entries', [{ ...definition.entries[0], completed: false }]],
  ['timezone_id', 'Asia/Shanghai'], ['schedule', { ...definition.schedule, interval: 0 }],
  ['schedule', { ...definition.schedule, anchor_date: '2026-02-30' }],
  ['schedule', { ...definition.schedule, extra: 0 }], ['deleted_at', 999]
]) invalid.push({ id: 'definition-field-' + field + '-' + invalid.length, kind: 'definitions',
  document: definitions({ ...definition, [field]: value }) });
const missingDefinition = clone(definition); delete missingDefinition.entries;
invalid.push({ id: 'missing-definition-entries', kind: 'definitions', document: definitions(missingDefinition) });
const merges = [];
function mergeCase(name, a, b, winner, kind = 'items') {
  merges.push({ id: name, kind, left: doc(kind, a), right: doc(kind, b), expected: doc(kind, winner) });
}
const dead = { ...item, updated_at: 2100, deleted_at: 2100 };
mergeCase('newer-row', item, { ...item, completed: true, updated_at: 2001 }, { ...item, completed: true, updated_at: 2001 });
mergeCase('deletion-beats-future-clock', { ...item, updated_at: Number.MAX_SAFE_INTEGER }, dead, dead);
mergeCase('device-tie', item, { ...item, updated_by: 'device-z' }, { ...item, updated_by: 'device-z' });
mergeCase('completed-tie', item, { ...item, completed: true }, { ...item, completed: true });
mergeCase('sort-tie', item, { ...item, sort_order: 2000 }, { ...item, sort_order: 2000 });
mergeCase('text-utf8-not-utf16', { ...item, content: '\ue000' }, { ...item, content: '🥚' }, { ...item, content: '🥚' });
mergeCase('same-record', item, item, item);
merges.push({ id: 'absence-not-delete', kind: 'items', left: items(item), right: items(), expected: items(item) });
const second = { ...item, uuid: id(11), content: 'Upload' };
merges.push({ id: 'different-items-concurrent-checks', kind: 'items',
  left: items({ ...item, completed: true, updated_at: 2001 }, second),
  right: items(item, { ...second, completed: true, updated_at: 2001 }),
  expected: items({ ...item, completed: true, updated_at: 2001 }, { ...second, completed: true, updated_at: 2001 }) });
merges.push({ id: 'same-id-different-parent', kind: 'items', left: items(item),
  right: items({ ...item, todo_uuid: id(999) }), error: 'ITEM_IDENTITY_CONFLICT' });
merges.push({ id: 'same-id-different-birth', kind: 'items', left: items(item),
  right: items({ ...item, created_at: 999 }), error: 'ITEM_IDENTITY_CONFLICT' });
mergeCase('same-definition', definition, definition, definition, 'definitions');
mergeCase('definition-deletion', { ...definition, updated_at: 9000 },
  { ...definition, updated_at: 2000, deleted_at: 2000 },
  { ...definition, updated_at: 2000, deleted_at: 2000 }, 'definitions');
merges.push({ id: 'immutable-definition-edit', kind: 'definitions', left: definitions(definition),
  right: definitions({ ...definition, entries: [{ ...definition.entries[0], content: 'Changed' }], updated_at: 5000 }),
  error: 'DEFINITION_IDENTITY_CONFLICT' });
merges.push({ id: 'definition-key-order', kind: 'definitions', left: definitions(definition),
  right: definitions({ ...definition, entries: [...definition.entries].reverse() }), expected: definitions(definition) });
const context = { verified: true, parent_exists: true, parent_deleted: false, parent_archived: false,
  todo_uuid: recurring.uuid, rule_uuid: rule, index: 2 };
const edited = generated.map((row, i) => i === 0 ? { ...row, completed: true, content: 'Edited', updated_at: 2000, updated_by: 'device-z' } :
  { ...row, deleted_at: 2000, updated_at: 2000, updated_by: 'device-z' });
const generation = [
  { id: 'old-client-parent-backfill', definition, existing: items(), context, state: 'ready', expected: items(...generated) },
  { id: 'preserve-check-edit-and-tombstone', definition, existing: items(...edited), context, state: 'ready', expected: items(...edited) },
  { id: 'missing-definition', definition: null, existing: items(), context, state: 'waiting', expected: items() },
  { id: 'missing-parent', definition, existing: items(), context: { ...context, parent_exists: false }, state: 'waiting', expected: items() },
  { id: 'unverified-context', definition, existing: items(), context: { ...context, verified: false }, state: 'waiting', expected: items() },
  { id: 'deleted-parent', definition, existing: items(...edited), context: { ...context, parent_deleted: true }, state: 'suppressed', expected: items(...edited) },
  { id: 'archived-parent', definition, existing: items(), context: { ...context, parent_archived: true }, state: 'suppressed', expected: items() },
  { id: 'first-task-not-overwritten', definition, existing: items(item), context: { ...context, todo_uuid: todo, index: 1 }, state: 'current-instance', expected: items(item) },
  { id: 'stopped-rule-existing-parent', definition, existing: items(), context: { ...context, rule_deleted: true }, state: 'ready', expected: items(...generated) },
  { id: 'different-rule-context', definition, existing: items(), context: { ...context, rule_uuid: id(888) }, error: 'CHECKLIST_CONTEXT_MISMATCH' },
  { id: 'identity-conflict-during-backfill', definition,
    existing: items({ ...generated[0], source_rule_uuid: id(888) }), context, error: 'ITEM_IDENTITY_CONFLICT' }
];
const v3 = JSON.parse(fs.readFileSync(path.join(root, 'docs/fixtures/task-note-link-backup-v3.json'), 'utf8'));
const v4 = { ...v3, format_version: 4, task_checklist_items: items(item), task_checklist_definitions: definitions(definition) };
const absent = clone(v4); delete absent.task_checklist_definitions;
const backups = [
  { id: 'v3-keeps-local-domain', input: v3, expected: null },
  { id: 'v4', input: v4, expected: { items: items(item), definitions: definitions(definition) } },
  { id: 'v4-empty', input: { ...v4, task_checklist_items: items(), task_checklist_definitions: definitions() },
    expected: { items: items(), definitions: definitions() } },
  { id: 'v3-disguised-extension', input: { ...v4, format_version: 3 }, error: true },
  { id: 'missing-extension', input: absent, error: true },
  { id: 'null-extension', input: { ...v4, task_checklist_items: null }, error: true },
  { id: 'unknown-version', input: { ...v4, format_version: 5 }, error: true },
  { id: 'unknown-new-field', input: { ...v4, templates: [] }, error: true }
];
const keys = [
  { todo: 'todos.json', kind: 'items', occupied: [], expected: 'task-checklist-items.json' },
  { todo: 'data/todos.json', kind: 'definitions', occupied: [], expected: 'data/task-checklist-definitions.json' },
  { todo: '资料/todos.json', kind: 'items', occupied: [], expected: '资料/task-checklist-items.json' },
  { todo: 'todos.json', kind: 'items', occupied: ['task-checklist-items.json'], error: true },
  { todo: '../todos.json', kind: 'items', occupied: [], error: true },
  { todo: 'data//todos.json', kind: 'items', occupied: [], error: true },
  { todo: 'a\\todos.json', kind: 'items', occupied: [], error: true },
  { todo: 'task-checklist-items.json', kind: 'items', occupied: [], error: true },
  { todo: 'a\u0000/todos.json', kind: 'items', occupied: [], error: true }
];
console.log(JSON.stringify({ contract_version: 1, base_item: item, base_definition: definition,
  identities: definition.entries.map(e => ({ todo: recurring.uuid, entry: e.uuid, expected: childId(recurring.uuid, e.uuid) })),
  recurrence_parent: { key: recurring.key, expected: recurring.uuid },
  valid, invalid, merges, generation, backups, keys }, null, 2));
