'use strict';
// LC2a executable prototype only. Neither application imports this module.
const { createHash } = require('node:crypto');
const PROTOCOL = 'eggdone.lifecycle.prototype.v1';
const MAX_BYTES = 2 * 1024 * 1024;
const MAX_ROWS = 1000;
const UUID = /^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
function fail(code) { throw new Error(code); }
function id(v) { if (typeof v !== 'string' || !UUID.test(v)) fail('PURGE_INVALID_ID'); }
function stamp(v) { if (!Number.isSafeInteger(v) || v < 0) fail('PURGE_INVALID_VERSION'); }
function object(v) { return v !== null && typeof v === 'object' && !Array.isArray(v); }
function keys(v, expected) {
  if (!object(v) || Object.keys(v).sort().join(',') !== expected.slice().sort().join(',')) fail('PURGE_INVALID_SHAPE');
}
function canonical(v, depth = 0) {
  if (depth > 32) fail('PURGE_TOO_DEEP');
  if (v === null || typeof v === 'string' || typeof v === 'boolean') return JSON.stringify(v);
  if (typeof v === 'number' && Number.isFinite(v)) return JSON.stringify(v);
  if (Array.isArray(v)) return '[' + v.map(x => canonical(x, depth + 1)).join(',') + ']';
  if (!object(v)) fail('PURGE_INVALID_JSON');
  return '{' + Object.keys(v).sort().map(k => JSON.stringify(k) + ':' + canonical(v[k], depth + 1)).join(',') + '}';
}
function fingerprint(v) { return createHash('sha256').update(canonical(v)).digest('hex'); }
function identity(v) { return v.kind + ':' + v.uuid; }
function entity(v) {
  if (!object(v)) fail('PURGE_INVALID_SHAPE');
  if (!['todo', 'note'].includes(v.kind)) fail('PURGE_INVALID_KIND');
  id(v.uuid);
}
function validate(doc, space) {
  id(space);
  keys(doc, ['protocol', 'space_id', 'records', 'terminals']);
  if (doc.protocol !== PROTOCOL || doc.space_id !== space) fail('PURGE_SCOPE_MISMATCH');
  for (const list of [doc.records, doc.terminals]) {
    if (!Array.isArray(list) || list.length > MAX_ROWS) fail('PURGE_LIMIT');
    const seen = new Set();
    for (const v of list) {
      entity(v);
      const key = identity(v);
      if (seen.has(key)) fail('PURGE_DUPLICATE');
      seen.add(key);
    }
  }
  for (const v of doc.records) {
    keys(v, ['kind', 'uuid', 'updated_at', 'deleted_at', 'body']);
    stamp(v.updated_at);
    if (v.deleted_at !== null) stamp(v.deleted_at);
    if (!object(v.body)) fail('PURGE_INVALID_BODY');
  }
  for (const v of doc.terminals) {
    keys(v, ['kind', 'uuid', 'operation_uuid', 'purged_at']);
    id(v.operation_uuid); stamp(v.purged_at);
  }
  if (Buffer.byteLength(canonical(doc), 'utf8') > MAX_BYTES) fail('PURGE_TOO_LARGE');
  return doc;
}
function empty(space) {
  id(space);
  return { protocol: PROTOCOL, space_id: space, records: [], terminals: [] };
}
function parse(text, space) {
  if (typeof text !== 'string' || Buffer.byteLength(text, 'utf8') > MAX_BYTES) fail('PURGE_TOO_LARGE');
  let value;
  try { value = JSON.parse(text); } catch { fail('PURGE_INVALID_JSON'); }
  return merge(empty(space), validate(value, space));
}
function merge(a, b) {
  validate(a, a.space_id); validate(b, a.space_id);
  const terminal = new Map();
  for (const v of [...a.terminals, ...b.terminals]) {
    const key = identity(v), old = terminal.get(key);
    // Presence is the terminal fact; canonical minimum only selects non-content provenance.
    if (!old || canonical(v) < canonical(old)) terminal.set(key, v);
  }
  const records = new Map();
  for (const v of [...a.records, ...b.records]) {
    const key = identity(v), old = records.get(key);
    if (terminal.has(key)) continue;
    if (!old || v.updated_at > old.updated_at ||
        (v.updated_at === old.updated_at && canonical(v) > canonical(old))) records.set(key, v);
  }
  const sorted = map => [...map.entries()].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([,v]) => structuredClone(v));
  return validate({ ...empty(a.space_id), records: sorted(records), terminals: sorted(terminal) }, a.space_id);
}
function purge(doc, expected, operation, now) {
  validate(doc, doc.space_id); id(operation); stamp(now); entity(expected);
  const existing = doc.terminals.find(v => identity(v) === identity(expected));
  if (existing) return merge(doc, empty(doc.space_id));
  const current = doc.records.find(v => identity(v) === identity(expected));
  if (!current || current.deleted_at === null || fingerprint(current) !== fingerprint(expected)) fail('PURGE_CONFLICT');
  return merge(doc, { ...empty(doc.space_id), terminals: [
    { kind: current.kind, uuid: current.uuid, operation_uuid: operation, purged_at: now }
  ] });
}
function restoreBackup(current, backup) {
  // Missing old-backup terminal data is not permission to clear local terminal evidence.
  const rows = { ...empty(current.space_id), records: structuredClone(backup) };
  return merge(current, rows);
}
function objectKey(space) {
  id(space);
  return 'account/lifecycle-prototype/v1/' + space + '/checkpoint.json';
}
async function sync(io, local, guard = () => {}) {
  validate(local, local.space_id);
  const key = objectKey(local.space_id);
  guard();
  const remote = await io.get(key);
  guard();
  if (remote === null) fail('PURGE_REMOTE_MISSING');
  if (!remote.etag) fail('PURGE_ETAG_REQUIRED');
  const merged = merge(local, parse(remote.text, local.space_id));
  guard();
  if (!await io.put(key, canonical(merged), remote.etag)) fail('PURGE_CAS_CONFLICT');
  // A config change or lost reply must not ACK a different local scope.
  guard();
  return merged;
}
function migration(sourceKey, sourceEtag, space, operation, seed) {
  if (sourceKey !== 'account/todos.json' || !sourceEtag) fail('PURGE_MIGRATION_SOURCE');
  id(operation);
  return { sourceKey, sourceEtag, space, operation, seed: canonical(merge(empty(space), validate(seed, space))) };
}
async function publish(io, plan, confirmed, guard = () => {}) {
  if (confirmed !== true) fail('PURGE_CONFIRM_REQUIRED');
  const seed = parse(plan.seed, plan.space);
  const commitKey = 'account/lifecycle-prototype/v1/migration.json';
  const commit = { protocol: PROTOCOL, space_id: plan.space, operation_uuid: plan.operation,
    source_key: plan.sourceKey, source_etag: plan.sourceEtag, seed_digest: fingerprint(seed) };
  const expected = canonical(commit);
  guard();
  const prior = await io.get(commitKey);
  guard();
  if (prior !== null) {
    if (prior.text !== expected) fail('PURGE_MIGRATION_CONFLICT');
    const installed = await io.get(objectKey(plan.space));
    guard();
    if (installed === null) fail('PURGE_REMOTE_MISSING');
    parse(installed.text, plan.space);
    return commit;
  }
  const source = await io.get(plan.sourceKey);
  guard();
  if (!source || source.etag !== plan.sourceEtag) fail('PURGE_SOURCE_CHANGED');
  const key = objectKey(plan.space);
  const staged = await io.get(key);
  guard();
  if (staged !== null) {
    if (staged.text !== plan.seed) fail('PURGE_STAGE_CONFLICT');
  } else {
    if (!await io.put(key, plan.seed, null)) fail('PURGE_CAS_CONFLICT');
    guard();
  }
  const recheck = await io.get(plan.sourceKey);
  guard();
  if (!recheck || recheck.etag !== plan.sourceEtag) fail('PURGE_SOURCE_CHANGED');
  if (!await io.put(commitKey, expected, null)) fail('PURGE_MIGRATION_CONFLICT');
  guard();
  // Caller commits its local target only after this read-back, never after staging.
  const durable = await io.get(commitKey);
  guard();
  if (!durable || durable.text !== expected) fail('PURGE_MIGRATION_CONFLICT');
  return commit;
}
module.exports = { PROTOCOL, MAX_BYTES, MAX_ROWS, empty, validate, parse, canonical, fingerprint,
  merge, purge, restoreBackup, objectKey, sync, migration, publish };
