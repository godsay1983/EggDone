'use strict';
// LC2a-2a host prototype. Fixed synthetic namespace; not imported by either app.
const { DatabaseSync } = require('node:sqlite');
const { createHash } = require('node:crypto');
const { canonical } = require('./purge-compat-prototype.cjs');
const FORMAT = 'eggdone.migration.fixture.v1';
const SCOPE = 'isolated-fixture-source';
const PREFIX = 'account/migration-prototype/v2/';
const COMMIT_KEY = PREFIX + 'migration.json';
const UUID = /^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const SHA = /^[0-9a-f]{64}$/;
const MAX_BYTES = 32 * 1024 * 1024;
const MAX_OBJECTS = 264;
const DOMAINS = Object.freeze([
  ['todos.json', ['groups', 'todos']],
  ['notes.json', ['notes']],
  ['note-attachments.json', ['attachments']],
  ['recurrence-rules.json', ['rules']],
  ['task-note-links.json', ['links']],
  ['task-checklist-items.json', ['items']],
  ['task-checklist-definitions.json', ['definitions']],
  ['task-templates.json', ['templates']]
].map(([name, fields]) => Object.freeze({ key: 'account/' + name, fields: Object.freeze(fields) })));
function requireThat(ok, code) { if (!ok) throw Error(code); }
function hash(bytes) { return createHash('sha256').update(bytes).digest('hex'); }
function json(bytes) { return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); }
function exact(value, keys) {
  requireThat(value && typeof value === 'object' && !Array.isArray(value) &&
    Object.keys(value).sort().join(',') === keys.slice().sort().join(','), 'MIGRATION_SHAPE');
}
function config(value) {
  exact(value, ['scope', 'epoch', 'revision', 'active']);
  requireThat(value.scope === SCOPE && Number.isSafeInteger(value.epoch) && value.epoch >= 0 &&
    value.epoch < Number.MAX_SAFE_INTEGER &&
    Number.isSafeInteger(value.revision) && value.revision >= 0 && value.active === 'legacy', 'MIGRATION_CONFIG');
}
function entry(key, result) {
  if (result === null) return { key, etag: null, sha256: null, size: 0 };
  requireThat(typeof result.etag === 'string' && result.etag.length > 0 && result.etag.length <= 1024 &&
    Buffer.isBuffer(result.bytes) && result.bytes.length <= MAX_BYTES, 'MIGRATION_OBJECT');
  return { key, etag: result.etag, sha256: hash(result.bytes), size: result.bytes.length };
}
function document(bytes, domain) {
  const doc = json(bytes);
  requireThat(doc && doc.format_version === 1, 'MIGRATION_FORMAT');
  for (const field of domain.fields) {
    // Legacy main documents may omit groups. Never rewrite the original bytes.
    const rows = field === 'groups' && doc.groups === undefined ? [] : doc[field];
    requireThat(Array.isArray(rows) && rows.length <= 10000, 'MIGRATION_ROWS');
  }
  return doc;
}
function metadata(read) {
  const counts = {}, assets = new Map();
  for (const domain of DOMAINS) {
    const bytes = read(domain.key);
    requireThat(bytes !== null || domain !== DOMAINS[0], 'MIGRATION_MAIN_MISSING');
    const doc = bytes === null ? null : document(bytes, domain);
    for (const field of domain.fields) counts[field] = doc?.[field]?.length ?? 0;
    if (!doc?.attachments) continue;
    const ids = new Set();
    for (const item of doc.attachments) {
      requireThat(item && UUID.test(item.uuid) && !ids.has(item.uuid), 'MIGRATION_ATTACHMENT_ID');
      ids.add(item.uuid);
      const add = (name, size, digest) => {
        requireThat(Number.isSafeInteger(size) && size >= 0 && size <= MAX_BYTES &&
          typeof digest === 'string' && SHA.test(digest), 'MIGRATION_ATTACHMENT_HASH');
        const key = 'account/note-assets/v1/' + item.uuid + '/' + name;
        assets.set(key, { key, size, sha256: digest });
      };
      add('original', item.byte_size, item.sha256);
      if (item.preview_sha256 !== null && item.preview_sha256 !== undefined) {
        add('preview.jpg', item.preview_byte_size, item.preview_sha256);
      } else {
        requireThat(item.preview_byte_size === null || item.preview_byte_size === undefined, 'MIGRATION_ATTACHMENT_HASH');
      }
    }
  }
  requireThat(assets.size + DOMAINS.length <= MAX_OBJECTS, 'MIGRATION_LIMIT');
  return { counts, assets: [...assets.values()].sort((a,b) => a.key.localeCompare(b.key, 'en')) };
}
function makePlan(expected, operation, space, entries, read) {
  config(expected);
  requireThat(UUID.test(operation) && UUID.test(space), 'MIGRATION_ID');
  const { counts, assets } = metadata(read);
  const orderedKeys = [...DOMAINS.map(d => d.key), ...assets.map(a => a.key)];
  requireThat(entries.length === orderedKeys.length, 'MIGRATION_TOPOLOGY');
  let total = 0;
  const missing = [];
  for (let i = 0; i < entries.length; i++) {
    const row = entries[i];
    exact(row, ['key', 'etag', 'sha256', 'size']);
    requireThat(row.key === orderedKeys[i], 'MIGRATION_TOPOLOGY');
    const bytes = read(row.key);
    requireThat(canonical(entry(row.key, bytes === null ? null : { etag: row.etag, bytes })) === canonical(row),
      'MIGRATION_BACKUP_CORRUPT');
    total += row.size;
    const asset = assets.find(a => a.key === row.key);
    if (asset) {
      if (bytes === null) missing.push(row.key);
      else requireThat(asset.size === row.size && asset.sha256 === row.sha256, 'MIGRATION_ASSET_CORRUPT');
    }
  }
  requireThat(total <= MAX_BYTES, 'MIGRATION_LIMIT');
  return { format: FORMAT, expected, operation, space, entries, counts, total_bytes: total, missing_assets: missing };
}
async function capture(io, expected, operation, space) {
  requireThat(io.scope === SCOPE, 'MIGRATION_SCOPE');
  const objects = new Map(), entries = [];
  let total = 0;
  const get = async key => {
    const result = await io.get(key);
    const row = entry(key, result);
    total += row.size;
    requireThat(total <= MAX_BYTES, 'MIGRATION_LIMIT');
    entries.push(row);
    objects.set(key, result === null ? null : Buffer.from(result.bytes));
  };
  for (const domain of DOMAINS) await get(domain.key);
  for (const asset of metadata(key => objects.get(key)).assets) await get(asset.key);
  const plan = makePlan(expected, operation, space, entries, key => objects.get(key));
  // Includes absent objects: missing -> present is also a version change.
  await recheck(io, plan);
  return { plan, objects };
}
async function recheck(io, plan) {
  for (const row of plan.entries) {
    requireThat(canonical(entry(row.key, await io.get(row.key))) === canonical(row), 'MIGRATION_SOURCE_CHANGED');
  }
}
function transaction(db, action) {
  db.exec('BEGIN IMMEDIATE');
  try { const value = action(); db.exec('COMMIT'); return value; }
  catch (error) { db.exec('ROLLBACK'); throw error; }
}
class Journal {
  constructor(file) {
    this.db = new DatabaseSync(file);
    this.db.exec('PRAGMA busy_timeout=2000; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;');
    this.db.exec(`CREATE TABLE IF NOT EXISTS local_target(id INTEGER PRIMARY KEY CHECK(id=1), value TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS migration(id INTEGER PRIMARY KEY CHECK(id=1), plan TEXT NOT NULL,
        digest TEXT NOT NULL, confirmed TEXT, phase TEXT NOT NULL CHECK(phase IN ('prepared','confirmed','complete')));
      CREATE TABLE IF NOT EXISTS backup(key TEXT PRIMARY KEY, bytes BLOB);
      CREATE TABLE IF NOT EXISTS progress(key TEXT PRIMARY KEY, digest TEXT NOT NULL);`);
  }
  initialize(value) {
    config(value);
    this.db.prepare('INSERT INTO local_target VALUES(1,?)').run(canonical(value));
  }
  target() {
    const row = this.db.prepare('SELECT value FROM local_target WHERE id=1').get();
    requireThat(row, 'MIGRATION_CONFIG');
    return JSON.parse(row.value);
  }
  guard(plan) {
    requireThat(canonical(this.target()) === canonical(plan.expected), 'MIGRATION_CONFIG_CHANGED');
  }
  prepare(snapshot) {
    const { plan, objects } = snapshot;
    const rebuilt = makePlan(plan.expected, plan.operation, plan.space, plan.entries, key => objects.get(key));
    requireThat(canonical(rebuilt) === canonical(plan), 'MIGRATION_PLAN');
    transaction(this.db, () => {
      this.guard(plan);
      requireThat(!this.db.prepare('SELECT id FROM migration').get(), 'MIGRATION_ALREADY_PREPARED');
      const text = canonical(plan);
      this.db.prepare('INSERT INTO migration VALUES(1,?,?,NULL,?)').run(text, hash(text), 'prepared');
      const insert = this.db.prepare('INSERT INTO backup VALUES(?,?)');
      for (const row of plan.entries) insert.run(row.key, objects.get(row.key));
    });
    return this.load();
  }
  load() {
    const job = this.db.prepare('SELECT * FROM migration WHERE id=1').get();
    requireThat(job && Buffer.byteLength(job.plan) <= 1024 * 1024 &&
      hash(job.plan) === job.digest, 'MIGRATION_JOURNAL_CORRUPT');
    const raw = JSON.parse(job.plan);
    exact(raw, ['format','expected','operation','space','entries','counts','total_bytes','missing_assets']);
    requireThat(Array.isArray(raw.entries) && raw.entries.length <= MAX_OBJECTS, 'MIGRATION_LIMIT');
    const limits = this.db.prepare('SELECT count(*) n, coalesce(sum(length(bytes)),0) size FROM backup').get();
    requireThat(limits.n <= MAX_OBJECTS && limits.size <= MAX_BYTES, 'MIGRATION_LIMIT');
    const objects = new Map(this.db.prepare('SELECT * FROM backup').all()
      .map(row => [row.key, row.bytes === null ? null : Buffer.from(row.bytes)]));
    requireThat(objects.size === raw.entries.length, 'MIGRATION_BACKUP_CORRUPT');
    const rebuilt = makePlan(raw.expected, raw.operation, raw.space, raw.entries, key => objects.get(key));
    requireThat(canonical(rebuilt) === job.plan, 'MIGRATION_PLAN');
    return { ...job, plan: rebuilt, objects };
  }
  confirm(digest) {
    transaction(this.db, () => {
      const job = this.load();
      this.guard(job.plan);
      requireThat(digest === job.digest, 'MIGRATION_CONFIRM_STALE');
      requireThat(job.plan.missing_assets.length === 0, 'MIGRATION_ASSETS_MISSING');
      this.db.prepare("UPDATE migration SET confirmed=?,phase='confirmed' WHERE id=1 AND phase!='complete'").run(digest);
    });
  }
  async switchTarget(job, at) {
    this.db.exec('BEGIN IMMEDIATE');
    try {
      this.guard(job.plan);
      const state = this.db.prepare('SELECT digest,confirmed FROM migration WHERE id=1').get();
      requireThat(state.digest === job.digest && state.confirmed === job.digest, 'MIGRATION_CONFIRM_REQUIRED');
      const next = { ...job.plan.expected, active: job.plan.space, epoch: job.plan.expected.epoch + 1 };
      this.db.prepare('UPDATE local_target SET value=? WHERE id=1').run(canonical(next));
      this.db.prepare("UPDATE migration SET phase='complete' WHERE id=1").run();
      await at('local-before-commit');
      this.db.exec('COMMIT');
    } catch (error) { this.db.exec('ROLLBACK'); throw error; }
    await at('local-after-commit');
  }
  close() { this.db.close(); }
}
function targetKey(plan, row) {
  return PREFIX + plan.space + '/objects/' + hash(row.key) + '/' + row.sha256;
}
async function run(io, journal, at = async () => {}) {
  requireThat(io.scope === SCOPE, 'MIGRATION_SCOPE');
  const job = journal.load(), plan = job.plan;
  requireThat(job.confirmed === job.digest, 'MIGRATION_CONFIRM_REQUIRED');
  requireThat(plan.missing_assets.length === 0, 'MIGRATION_ASSETS_MISSING');
  const published = Buffer.from(canonical({ format: FORMAT, plan_sha256: job.digest, plan }));
  if (job.phase === 'complete') {
    // Completed retry is read-only, and cannot undo later edits/configuration.
    return { phase: 'complete', space: plan.space, already_complete: true };
  }
  const guard = () => journal.guard(plan);
  guard();
  const prior = await io.get(COMMIT_KEY);
  guard();
  requireThat(prior === null || prior.bytes.equals(published), 'MIGRATION_PUBLISH_CONFLICT');
  if (prior === null) { await recheck(io, plan); guard(); }
  for (let i = 0; i < plan.entries.length; i++) {
    const row = plan.entries[i];
    if (row.etag === null) continue;
    guard();
    const key = targetKey(plan, row), bytes = job.objects.get(row.key);
    let remote = await io.get(key);
    guard();
    if (remote === null) {
      // A published snapshot cannot be silently repaired from a possibly obsolete backup.
      requireThat(prior === null, 'MIGRATION_TARGET_MISSING');
      await at('object-' + i + '-before');
      requireThat(await io.create(key, bytes), 'MIGRATION_STAGE_CONFLICT');
      await at('object-' + i + '-after');
      guard();
      remote = await io.get(key);
    }
    requireThat(remote && hash(remote.bytes) === row.sha256 && remote.bytes.length === row.size,
      'MIGRATION_TARGET_CORRUPT');
    guard();
    journal.db.prepare('INSERT OR REPLACE INTO progress VALUES(?,?)').run(key, row.sha256);
  }
  if (prior === null) {
    await recheck(io, plan);
    guard();
    await at('publish-before');
    guard();
    requireThat(await io.create(COMMIT_KEY, published), 'MIGRATION_PUBLISH_CONFLICT');
    await at('publish-after');
    guard();
  }
  const proof = await io.get(COMMIT_KEY);
  requireThat(proof && proof.bytes.equals(published), 'MIGRATION_PUBLISH_CONFLICT');
  guard();
  for (const row of plan.entries) {
    if (row.etag === null) continue;
    const installed = await io.get(targetKey(plan, row));
    requireThat(installed && installed.bytes.length === row.size &&
      hash(installed.bytes) === row.sha256, 'MIGRATION_TARGET_CORRUPT');
    guard();
  }
  await at('switch-before');
  await journal.switchTarget(job, at);
  return { phase: 'complete', space: plan.space, already_complete: false };
}
module.exports = { DOMAINS, SCOPE, PREFIX, COMMIT_KEY, MAX_BYTES, hash, capture, Journal, run, targetKey };
