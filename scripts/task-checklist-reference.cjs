// P1a executable specification only. Production Rust/ArkTS adapters must run the same fixtures.
const { createHash } = require('node:crypto');
const MAX = Number.MAX_SAFE_INTEGER;
const LIMIT = 4 * 1024 * 1024;
const ITEM_KEYS = ['uuid', 'todo_uuid', 'source_rule_uuid', 'source_entry_uuid', 'content',
  'sort_order', 'completed', 'created_at', 'updated_at', 'updated_by', 'deleted_at'];
const DEFINITION_KEYS = ['rule_uuid', 'first_todo_uuid', 'schedule', 'timezone_id',
  'applies_from_index', 'entries', 'created_at', 'updated_at', 'updated_by', 'deleted_at'];
const clone = value => JSON.parse(JSON.stringify(value));
function fail(code = 'INVALID_CHECKLIST_DOCUMENT') { throw new Error(code); }
function exact(value, keys) {
  if (!value || typeof value !== 'object' || Array.isArray(value) ||
      Object.keys(value).length !== keys.length || Object.keys(value).some(k => !keys.includes(k))) fail();
}
function uuid(value) {
  return typeof value === 'string' && value.length === 36 &&
    /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
}
function integer(n, min = 0, max = MAX) { return Number.isSafeInteger(n) && n >= min && n <= max; }
function content(text) {
  if (typeof text !== 'string' || text.length < 1 || text.length > 200 || text.trim() !== text ||
      /[\u0000-\u001f\u007f-\u009f\u2028-\u202e\u2066-\u2069]/.test(text)) return false;
  for (let i = 0; i < text.length; i++) {
    const n = text.charCodeAt(i);
    if (n >= 0xd800 && n <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return false;
    } else if (n >= 0xdc00 && n <= 0xdfff) return false;
  }
  return true;
}
function stamp(row) {
  if (!integer(row.created_at) || !integer(row.updated_at, row.created_at) ||
      typeof row.updated_by !== 'string' || row.updated_by.length < 1 || row.updated_by.length > 128 ||
      /[^A-Za-z0-9._:-]/.test(row.updated_by) ||
      (row.deleted_at !== null && !integer(row.deleted_at, row.created_at, row.updated_at))) fail();
}
function uuidV5(name) {
  const hash = createHash('sha1').update(Buffer.from('6ba7b8109dad11d180b400c04fd430c8', 'hex'))
    .update(name, 'utf8').digest().subarray(0, 16);
  hash[6] = (hash[6] & 15) | 80;
  hash[8] = (hash[8] & 63) | 128;
  const s = hash.toString('hex');
  return [s.slice(0, 8), s.slice(8, 12), s.slice(12, 16), s.slice(16, 20), s.slice(20)].join('-');
}
function itemUuid(todo, entry) {
  if (!uuid(todo) || !uuid(entry)) fail('INVALID_CHECKLIST_IDENTITY');
  return uuidV5('eggdone:task-checklist-item:v1:' + todo + ':' + entry);
}
function validateItem(row) {
  exact(row, ITEM_KEYS);
  if (!uuid(row.uuid) || !uuid(row.todo_uuid) || !content(row.content) ||
      !integer(row.sort_order) || typeof row.completed !== 'boolean') fail();
  const manual = row.source_rule_uuid === null && row.source_entry_uuid === null;
  if (!manual && (!uuid(row.source_rule_uuid) || !uuid(row.source_entry_uuid) ||
      row.uuid !== itemUuid(row.todo_uuid, row.source_entry_uuid))) fail();
  stamp(row);
}
function date(value) {
  if (typeof value !== 'string' || !/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/.test(value) || value.length !== 10 ||
      value < '1900-01-01' || value > '9999-12-31') return false;
  const n = Date.parse(value + 'T00:00:00Z');
  return Number.isFinite(n) && new Date(n).toISOString().slice(0, 10) === value;
}
// Structural schedule checks only; occurrence membership remains the existing recurrence engine's job.
function schedule(value) {
  exact(value, ['anchor_date', 'frequency', 'interval', 'weekdays', 'month_day', 'end_type',
    'end_date', 'max_occurrences', 'local_time_minutes']);
  if (!date(value.anchor_date) || !['daily', 'weekly', 'monthly'].includes(value.frequency) ||
      !integer(value.interval, 1, 99) || !Array.isArray(value.weekdays) ||
      value.weekdays.some((n, i) => !integer(n, 1, 7) || (i > 0 && value.weekdays[i - 1] >= n)) ||
      (value.local_time_minutes !== null && !integer(value.local_time_minutes, 0, 1439))) fail();
  if (value.frequency === 'weekly' && (value.weekdays.length < 1 || value.weekdays.length > 7 || value.month_day !== null)) fail();
  if (value.frequency === 'monthly' && (!integer(value.month_day, 0, 31) || value.weekdays.length)) fail();
  if (value.frequency === 'daily' && (value.weekdays.length || value.month_day !== null)) fail();
  if (!['never', 'date', 'count'].includes(value.end_type)) fail();
  if (value.end_type === 'never' && (value.end_date !== null || value.max_occurrences !== null)) fail();
  if (value.end_type === 'date' && (!date(value.end_date) || value.end_date < value.anchor_date || value.max_occurrences !== null)) fail();
  if (value.end_type === 'count' && (!integer(value.max_occurrences, 1, 100000) || value.end_date !== null)) fail();
}
function validateDefinition(row) {
  exact(row, DEFINITION_KEYS);
  if (!uuid(row.rule_uuid) || !uuid(row.first_todo_uuid) || row.applies_from_index !== 2 ||
      !Array.isArray(row.entries) || row.entries.length > 1000) fail();
  schedule(row.schedule);
  if (row.schedule.local_time_minutes === null ? row.timezone_id !== null :
    (typeof row.timezone_id !== 'string' || row.timezone_id.length < 1 || row.timezone_id.length > 128 ||
      /[^A-Za-z0-9_+\-/]/.test(row.timezone_id) || row.timezone_id.startsWith('/') ||
      row.timezone_id.endsWith('/') || row.timezone_id.includes('//'))) fail();
  const ids = new Set();
  for (const item of row.entries) {
    exact(item, ['uuid', 'content', 'sort_order']);
    if (!uuid(item.uuid) || !content(item.content) || !integer(item.sort_order) || ids.has(item.uuid)) fail();
    ids.add(item.uuid);
  }
  stamp(row);
}
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort().map(k => [k, canonical(value[k])]));
  return value;
}
function stable(value) { return JSON.stringify(canonical(value)); }
function checkDocument(doc, kind) {
  if (!['items', 'definitions'].includes(kind)) fail();
  exact(doc, ['format_version', kind]);
  if (doc.format_version !== 1 || !Array.isArray(doc[kind]) || doc[kind].length > (kind === 'items' ? 10000 : 2000)) fail();
  const ids = new Set();
  let entries = 0;
  for (const row of doc[kind]) {
    if (kind === 'items') validateItem(row); else { validateDefinition(row); entries += row.entries.length; }
    const id = kind === 'items' ? row.uuid : row.rule_uuid;
    if (ids.has(id)) fail();
    ids.add(id);
  }
  if (entries > 20000 || Buffer.byteLength(JSON.stringify(doc), 'utf8') > LIMIT) fail();
  return doc;
}
function parse(text, kind) {
  if (typeof text !== 'string' || Buffer.byteLength(text, 'utf8') > LIMIT) fail();
  return checkDocument(JSON.parse(text), kind);
}
function immutable(row, kind) {
  if (kind === 'items') return [row.todo_uuid, row.source_rule_uuid, row.source_entry_uuid, row.created_at];
  return [row.rule_uuid, row.first_todo_uuid, row.schedule, row.timezone_id,
    row.applies_from_index, [...row.entries].sort((a, b) => a.uuid < b.uuid ? -1 : a.uuid > b.uuid ? 1 : 0), row.created_at];
}
function rank(row, kind) {
  const result = [row.deleted_at !== null ? 1 : 0, row.updated_at, row.updated_by, row.deleted_at ?? -1];
  if (kind === 'items') result.push(row.completed ? 1 : 0, row.sort_order, row.content);
  return result;
}
function compare(a, b) {
  for (let i = 0; i < a.length; i++) {
    const c = typeof a[i] === 'string' ? Buffer.compare(Buffer.from(a[i], 'utf8'), Buffer.from(b[i], 'utf8')) :
      (a[i] > b[i] ? 1 : a[i] < b[i] ? -1 : 0);
    if (c) return c;
  }
  return 0;
}
function merge(a, b, kind) {
  checkDocument(a, kind); checkDocument(b, kind);
  const rows = new Map();
  for (const row of [...a[kind], ...b[kind]]) {
    const id = kind === 'items' ? row.uuid : row.rule_uuid;
    const previous = rows.get(id);
    if (previous && stable(immutable(previous, kind)) !== stable(immutable(row, kind))) {
      fail(kind === 'items' ? 'ITEM_IDENTITY_CONFLICT' : 'DEFINITION_IDENTITY_CONFLICT');
    }
    if (!previous || compare(rank(row, kind), rank(previous, kind)) > 0) rows.set(id, row);
  }
  const output = [...rows.values()].map(clone);
  if (kind === 'definitions') output.forEach(row => row.entries.sort((a, b) => a.uuid < b.uuid ? -1 : 1));
  output.sort((a, b) => {
    const x = kind === 'items' ? a.uuid : a.rule_uuid, y = kind === 'items' ? b.uuid : b.rule_uuid;
    return x < y ? -1 : x > y ? 1 : 0;
  });
  return checkDocument({ format_version: 1, [kind]: output }, kind);
}
function encode(doc, kind) { return stable(merge(doc, { format_version: 1, [kind]: [] }, kind)); }
function objectKey(todoKey, kind, occupied = []) {
  if (!['items', 'definitions'].includes(kind) || typeof todoKey !== 'string' ||
      !todoKey || todoKey.trim() !== todoKey || Buffer.byteLength(todoKey, 'utf8') > 1024 ||
      /[\\\u0000-\u001f\u007f]/.test(todoKey) || Buffer.from(todoKey).toString('utf8') !== todoKey ||
      todoKey.split('/').some(p => !p || p === '.' || p === '..')) fail('INVALID_CHECKLIST_KEY');
  const key = todoKey.slice(0, todoKey.lastIndexOf('/') + 1) + 'task-checklist-' + kind + '.json';
  if (key === todoKey || occupied.includes(key) || Buffer.byteLength(key, 'utf8') > 1024) fail('CHECKLIST_KEY_COLLISION');
  return key;
}
// Context is supplied only after production recurrence/parent validation. Never derives dates or creates a Todo.
function materialize(definition, existing, context) {
  checkDocument(existing, 'items');
  if (definition === null) return { state: 'waiting', document: clone(existing) };
  validateDefinition(definition);
  if (definition.deleted_at !== null || context.parent_deleted || context.parent_archived) return { state: 'suppressed', document: clone(existing) };
  if (!context.verified || !context.parent_exists) return { state: 'waiting', document: clone(existing) };
  if (!uuid(context.todo_uuid) || context.rule_uuid !== definition.rule_uuid ||
      !integer(context.index, 1, 100000)) fail('CHECKLIST_CONTEXT_MISMATCH');
  if (context.index < definition.applies_from_index) return { state: 'current-instance', document: clone(existing) };
  const candidates = [];
  for (const entry of definition.entries) {
    const candidate = { uuid: itemUuid(context.todo_uuid, entry.uuid), todo_uuid: context.todo_uuid,
      source_rule_uuid: definition.rule_uuid, source_entry_uuid: entry.uuid,
      content: entry.content, sort_order: entry.sort_order, completed: false,
      created_at: definition.created_at, updated_at: definition.created_at,
      updated_by: 'checklist-seed-v1', deleted_at: null };
    const previous = existing.items.find(item => item.uuid === candidate.uuid);
    if (previous) {
      if (stable(immutable(previous, 'items')) !== stable(immutable(candidate, 'items'))) fail('ITEM_IDENTITY_CONFLICT');
    } else candidates.push(candidate);
  }
  return { state: 'ready', document: merge(existing, { format_version: 1, items: candidates }, 'items') };
}
// Extension gate only. The existing data-exchange parser must still validate every legacy field and asset.
function backupExtensions(backup) {
  if (!backup || typeof backup !== 'object' || Array.isArray(backup) || !integer(backup.format_version, 1, 4)) fail('INVALID_CHECKLIST_BACKUP');
  const a = 'task_checklist_items', b = 'task_checklist_definitions';
  if (backup.format_version < 4) {
    if (Object.hasOwn(backup, a) || Object.hasOwn(backup, b)) fail('INVALID_CHECKLIST_BACKUP');
    return null;
  }
  const known = ['format_version', 'exported_at', 'groups', 'todos', 'notes', 'note_attachments',
    'attachment_files_included', 'recurrence', 'task_note_links', a, b];
  if (Object.keys(backup).some(k => !known.includes(k)) || !Object.hasOwn(backup, a) || !Object.hasOwn(backup, b)) fail('INVALID_CHECKLIST_BACKUP');
  checkDocument(backup[a], 'items'); checkDocument(backup[b], 'definitions');
  return { items: backup[a], definitions: backup[b] };
}
module.exports = { itemUuid, uuidV5, parse, encode, merge, validateItem, validateDefinition,
  checkDocument, objectKey, materialize, backupExtensions };
