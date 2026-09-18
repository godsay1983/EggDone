'use strict';
// Synthetic object store and host SQLite journals only. No network or application database access.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn } = require('node:child_process');
const { DatabaseSync } = require('node:sqlite');
const { randomUUID } = require('node:crypto');
const p = require('./migration-journal-prototype.cjs');
const SPACE = '123e4567-e89b-42d3-a456-426614174070';
const OP = '123e4567-e89b-42d3-a456-426614174080';
const NOTE = '123e4567-e89b-42d3-a456-426614174010';
const ASSET = '123e4567-e89b-42d3-a456-426614174020';
const EXPECTED = { scope: p.SCOPE, binding: p.hash('synthetic-host-store'), epoch: 1, revision: 7, active: 'legacy' };
const assetKey = name => 'account/note-assets/v1/' + ASSET + '/' + name;
function owned(root) {
  const absolute = fs.realpathSync(root);
  assert.equal(path.dirname(absolute).toLowerCase(), fs.realpathSync(os.tmpdir()).toLowerCase());
  assert(path.basename(absolute).startsWith('eggdone-migration-'));
  assert.equal(fs.readFileSync(path.join(absolute, 'fixture-owner'), 'utf8'), 'migration-host-test-only');
  return absolute;
}
class Remote {
  constructor(root) {
    this.scope = p.SCOPE;
    this.binding = EXPECTED.binding;
    this.db = new DatabaseSync(path.join(owned(root), 'remote.sqlite'));
    this.db.exec('PRAGMA synchronous=FULL; PRAGMA busy_timeout=2000; CREATE TABLE IF NOT EXISTS objects(key TEXT PRIMARY KEY, bytes BLOB NOT NULL, etag TEXT NOT NULL)');
  }
  put(key, bytes) {
    this.db.prepare('INSERT OR REPLACE INTO objects VALUES(?,?,?)').run(key, bytes, randomUUID());
  }
  async get(key) {
    const row = this.db.prepare('SELECT * FROM objects WHERE key=?').get(key);
    return row ? { etag: row.etag, bytes: Buffer.from(row.bytes) } : null;
  }
  async create(key, bytes) {
    assert(key.startsWith(p.PREFIX), 'prototype must never write source namespace');
    return this.db.prepare('INSERT OR IGNORE INTO objects VALUES(?,?,?)').run(key, bytes, randomUUID()).changes === 1;
  }
  countTargets() {
    return this.db.prepare('SELECT count(*) n FROM objects WHERE key LIKE ?').get(p.PREFIX + '%').n;
  }
  close() { this.db.close(); }
}
function seed(io) {
  const original = Buffer.from([0, 255, 23, 46, 0, 128]);
  const preview = Buffer.from('synthetic-preview');
  const fields = {
    groups: [{ uuid: 'group-fixture', name: 'Private fixture group' }],
    todos: [{ uuid: OP, title: 'Original title', archived_at: 100, completed: true }],
    notes: [{ uuid: NOTE, content: 'Preserve raw note and unknown fields', deleted_at: null }],
    attachments: [{ uuid: ASSET, note_uuid: NOTE, byte_size: original.length, sha256: p.hash(original),
      preview_byte_size: preview.length, preview_sha256: p.hash(preview), deleted_at: null }],
    rules: [{ uuid: SPACE, current_todo_uuid: OP }],
    links: [{ uuid: ASSET, todo_uuid: OP, note_uuid: NOTE, active: true }],
    items: [{ uuid: NOTE, todo_uuid: OP, completed: true, content: 'Checked fixture item' }],
    definitions: [{ rule_uuid: SPACE, first_todo_uuid: OP, entries: [] }],
    templates: [{ uuid: NOTE, content: { name: 'Independent fixture template' } }]
  };
  for (const domain of p.DOMAINS) {
    const doc = { format_version: 1, fixture_extension: { preserve: true } };
    for (const field of domain.fields) doc[field] = fields[field];
    io.put(domain.key, Buffer.from(JSON.stringify(doc, null, 2)));
  }
  io.put(assetKey('original'), original);
  io.put(assetKey('preview.jpg'), preview);
}
const roots = [];
async function setup(change, confirm = true) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'eggdone-migration-'));
  fs.writeFileSync(path.join(root,'fixture-owner'), 'migration-host-test-only', { flag: 'wx' });
  roots.push(root);
  const io = new Remote(root), journal = new p.Journal(path.join(root, 'journal.sqlite'));
  journal.initialize(EXPECTED);
  // Local-only data must survive target switching; no history is published as cloud data.
  journal.db.exec("CREATE TABLE local_history(id INTEGER PRIMARY KEY, content TEXT); INSERT INTO local_history VALUES(1,'local history sentinel')");
  seed(io);
  try {
    if (change) await change(io, journal);
    const snapshot = await p.capture(io, EXPECTED, OP, SPACE);
    const job = journal.prepare(snapshot);
    if (confirm) journal.confirm(job.digest);
    return { root, io, journal, job, snapshot, close: () => { journal.close(); io.close(); } };
  } catch (error) { journal.close(); io.close(); throw error; }
}
function worker(root, point) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [__filename, '--worker', owned(root), point],
      { stdio: ['ignore','pipe','pipe','ipc'], windowsHide: true });
    let stopped = false, output = '';
    child.stdout.on('data', data => output += data);
    child.stderr.on('data', data => output += data);
    const timer = setTimeout(() => { child.kill('SIGKILL'); reject(Error('Worker timeout: ' + point + '\n' + output)); }, 15000);
    child.on('error', error => { clearTimeout(timer); reject(error); });
    child.on('message', message => {
      if (message.paused === point && point !== 'none') {
        stopped = true;
        child.kill('SIGKILL');
      }
    });
    child.on('exit', code => {
      clearTimeout(timer);
      if (point === 'none' ? code === 0 : stopped && code !== 0) resolve();
      else reject(Error('Worker failed at ' + point + ': ' + code + '\n' + output));
    });
  });
}
async function workerMain() {
  const root = owned(process.argv[3]), point = process.argv[4];
  const io = new Remote(root), journal = new p.Journal(path.join(root,'journal.sqlite'));
  try {
    await p.run(io, journal, async name => {
      if (name === point) {
        process.send({ paused: name });
        await new Promise(() => {});
      }
    });
  } finally { journal.close(); io.close(); }
}
let checks = 0, killed = 0;
async function test(name, action) {
  try { await action(); checks++; }
  catch (cause) { throw new Error(name, { cause }); }
}
async function main() {
  await test('all eight objects, nine domains and binary assets preserved', async () => {
    const t = await setup();
    try {
      assert.equal(Object.keys(t.job.plan.counts).length, 9);
      assert(Object.values(t.job.plan.counts).every(v => v === 1));
      assert.equal(t.job.plan.entries.length, 10);
      assert.equal(t.job.plan.missing_assets.length, 0);
      assert.equal(t.journal.target().active, 'legacy');
      await p.run(t.io, t.journal);
      assert.equal(t.journal.target().active, SPACE);
      assert.equal(t.journal.load().phase, 'complete');
      for (const row of t.job.plan.entries) {
        const source = await t.io.get(row.key), target = await t.io.get(p.targetKey(t.job.plan,row));
        assert.equal(source.etag,row.etag);
        assert.deepEqual(target.bytes,source.bytes);
      }
      assert.equal(t.journal.db.prepare('SELECT content FROM local_history').get().content, 'local history sentinel');
      const targetCount = t.io.countTargets();
      assert.equal((await p.run(t.io,t.journal)).already_complete,true);
      assert.equal(t.io.countTargets(),targetCount);
    } finally { t.close(); }
  });
  await test('prepare/preview is local-only and requires exact persisted confirmation', async () => {
    const t = await setup(null,false);
    try {
      assert.equal(t.io.countTargets(),0);
      await assert.rejects(p.run(t.io,t.journal),/CONFIRM_REQUIRED/);
      assert.throws(()=>t.journal.confirm('different-preview'),/CONFIRM_STALE/);
      t.journal.confirm(t.job.digest);
      await p.run(t.io,t.journal);
    } finally { t.close(); }
  });
  await test('missing asset visible in preview and blocks confirmation, even for trash', async () => {
    const t = await setup(async io=>{
      io.db.prepare('DELETE FROM objects WHERE key=?').run(assetKey('original'));
      const key='account/note-attachments.json',doc=JSON.parse((await io.get(key)).bytes);
      doc.attachments[0].deleted_at=100;
      io.put(key,Buffer.from(JSON.stringify(doc)));
    },false);
    try {
      assert.deepEqual(t.job.plan.missing_assets,[assetKey('original')]);
      assert.throws(()=>t.journal.confirm(t.job.digest),/ASSETS_MISSING/);
      assert.equal(t.io.countTargets(),0);
    } finally { t.close(); }
  });
  await test('optional absent domain tracked, cannot appear after preview unnoticed', async () => {
    const key='account/task-templates.json';
    const t = await setup(io=>io.db.prepare('DELETE FROM objects WHERE key=?').run(key));
    try {
      assert.equal(t.job.plan.counts.templates,0);
      assert.equal(t.job.plan.entries.find(r=>r.key===key).etag,null);
      t.io.put(key,Buffer.from('{"format_version":1,"templates":[]}'));
      await assert.rejects(p.run(t.io,t.journal),/SOURCE_CHANGED/);
      assert.equal(t.io.countTargets(),0);
    } finally { t.close(); }
  });
  await test('metadata and attachment integrity reject invalid capture', async () => {
    for (const [key,bytes,error] of [
      ['account/todos.json',Buffer.from('{"format_version":2,"todos":[]}'),/FORMAT/],
      [assetKey('original'),Buffer.from('wrong bytes'),/ASSET_CORRUPT/],
      ['account/notes.json',Buffer.from('broken JSON'),/JSON/]
    ]) await assert.rejects(setup(io=>io.put(key,bytes)),error);
    await assert.rejects(setup(io=>io.db.prepare("DELETE FROM objects WHERE key='account/todos.json'").run()),/MAIN_MISSING/);
  });
  await test('incomplete capture or source mutation cannot create a prepared journal', async () => {
    const t=await setup();
    try {
      let calls=0;
      const moving={scope:p.SCOPE,binding:EXPECTED.binding,get:async key=>{
        const result=await t.io.get(key);
        if(++calls===8) t.io.put('account/notes.json',Buffer.from('{"format_version":1,"notes":[]}'));
        return result;
      }};
      await assert.rejects(p.capture(moving,EXPECTED,OP,SPACE),/SOURCE_CHANGED/);
    } finally {t.close();}
  });
  await test('corrupt backup/plan fail before writes, progress is not trusted proof', async () => {
    for(const kind of ['backup','plan']) {
      const t=await setup();
      try {
        if(kind==='backup') t.journal.db.prepare("UPDATE backup SET bytes=? WHERE key='account/notes.json'").run(Buffer.from('{}'));
        else t.journal.db.prepare("UPDATE migration SET plan='{}'").run();
        await assert.rejects(p.run(t.io,t.journal),/CORRUPT|FORMAT/);
        assert.equal(t.io.countTargets(),0);
      } finally {t.close();}
    }
  });
  await test('config or local revision changes never switch the new local target', async () => {
    for(const patch of [{epoch:2},{revision:8},{scope:'different-space'}]) {
      const t=await setup();
      try {
        t.journal.db.prepare('UPDATE local_target SET value=?').run(JSON.stringify({...EXPECTED,...patch}));
        await assert.rejects(p.run(t.io,t.journal),/CONFIG_CHANGED/);
        assert.equal(t.io.countTargets(),0);
      } finally {t.close();}
    }
  });
  await test('lost publication reply resumes against existing proof without changing old space', async () => {
    const t=await setup();
    try {
      let lost=true;
      const io={scope:p.SCOPE,binding:EXPECTED.binding,get:t.io.get.bind(t.io),create:async(key,bytes)=>{
        const created=await t.io.create(key,bytes);
        if(key===p.COMMIT_KEY && lost){lost=false;throw Error('REPLY_LOST');}
        return created;
      }};
      await assert.rejects(p.run(io,t.journal),/REPLY_LOST/);
      assert.equal(t.journal.target().active,'legacy');
      // A late source write stays in the old space and must not reset the committed seed.
      t.io.put('account/todos.json',Buffer.from('{"format_version":1,"todos":[{"title":"late"}]}'));
      await p.run(t.io,t.journal);
      assert.equal(t.journal.target().active,SPACE);
      assert.equal(JSON.parse((await t.io.get('account/todos.json')).bytes).todos[0].title,'late');
    } finally {t.close();}
  });
  await test('published snapshot missing data cannot be silently recreated on resume', async () => {
    const t=await setup();
    try {
      await assert.rejects(p.run(t.io,t.journal,async point=>{
        if(point==='publish-after')throw Error('STOP');
      }),/STOP/);
      const key=p.targetKey(t.job.plan,t.job.plan.entries[0]);
      t.io.db.prepare('DELETE FROM objects WHERE key=?').run(key);
      await assert.rejects(p.run(t.io,t.journal),/TARGET_MISSING/);
      assert.equal(t.journal.target().active,'legacy');
    } finally {t.close();}
  });
  await test('another migration cannot overwrite an existing publication', async () => {
    const t=await setup();
    try {
      t.io.put(p.COMMIT_KEY,Buffer.from('foreign migration'));
      await assert.rejects(p.run(t.io,t.journal),/PUBLISH_CONFLICT/);
      assert.equal(t.io.countTargets(),1);
    } finally {t.close();}
  });
  await test('source change after staging or target corruption blocks publication', async () => {
    for(const mode of ['source','target']){
      const t=await setup();
      try {
        await assert.rejects(p.run(t.io,t.journal,async point=>{
          if(point==='object-0-after'){
            if(mode==='source') t.io.put('account/notes.json',Buffer.from('{"format_version":1,"notes":[]}'));
            else t.io.put(p.targetKey(t.job.plan,t.job.plan.entries[0]),Buffer.from('corrupt'));
          }
        }),mode==='source'?/SOURCE_CHANGED/:/TARGET_CORRUPT/);
        assert.equal(await t.io.get(p.COMMIT_KEY),null);
      } finally {t.close();}
    }
  });
  await test('configuration change after publication refuses local switch', async () => {
    const t=await setup();
    try {
      await assert.rejects(p.run(t.io,t.journal,async point=>{
        if(point==='publish-after')t.journal.db.prepare('UPDATE local_target SET value=?')
          .run(JSON.stringify({...EXPECTED,revision:8}));
      }),/CONFIG_CHANGED/);
      assert.equal(t.journal.target().active,'legacy');
      assert(await t.io.get(p.COMMIT_KEY));
    } finally {t.close();}
  });
  await test('absent secondary domains remain absent without inventing empty source objects',async()=>{
    const t=await setup(io=>io.db.prepare("DELETE FROM objects WHERE key='account/task-templates.json'").run());
    try {
      await p.run(t.io,t.journal);
      assert.equal(await t.io.get('account/task-templates.json'),null);
      assert.equal(t.io.countTargets(),10);
    } finally {t.close();}
  });
  await test('foreign scope, read permission errors and unsafe counters fail closed',async()=>{
    const t=await setup();
    try {
      await assert.rejects(p.capture({scope:'foreign'},EXPECTED,OP,SPACE),/SCOPE/);
      await assert.rejects(p.capture({scope:p.SCOPE,binding:EXPECTED.binding,get:async()=>{throw Error('FORBIDDEN');}},EXPECTED,OP,SPACE),/FORBIDDEN/);
      await assert.rejects(p.capture(t.io,{...EXPECTED,epoch:Number.MAX_SAFE_INTEGER},OP,SPACE),/CONFIG/);
      assert.equal(t.io.countTargets(),0);
    } finally {t.close();}
  });
  await test('stale progress cannot bypass target verification and metadata cannot be omitted',async()=>{
    const t=await setup();
    try {
      const row=t.job.plan.entries[0],key=p.targetKey(t.job.plan,row);
      t.journal.db.prepare('INSERT INTO progress VALUES(?,?)').run(key,row.sha256);
      t.io.put(key,Buffer.from('corrupt staged payload'));
      await assert.rejects(p.run(t.io,t.journal),/TARGET_CORRUPT/);
      t.journal.db.prepare("DELETE FROM backup WHERE key='account/notes.json'").run();
      await assert.rejects(p.run(t.io,t.journal),/BACKUP_CORRUPT/);
      assert.equal(await t.io.get(p.COMMIT_KEY),null);
    } finally {t.close();}
  });
  await test('post-publication target corruption or pre-publication config change cannot switch',async()=>{
    for(const mode of ['corruption','configuration']){
      const t=await setup();
      try {
        await assert.rejects(p.run(t.io,t.journal,async point=>{
          if(mode==='corruption' && point==='publish-after')
            t.io.put(p.targetKey(t.job.plan,t.job.plan.entries[0]),Buffer.from('damaged after verify'));
          if(mode==='configuration' && point==='publish-before')
            t.journal.db.prepare('UPDATE local_target SET value=?').run(JSON.stringify({...EXPECTED,epoch:2}));
        }),mode==='corruption'?/TARGET_CORRUPT/:/CONFIG_CHANGED/);
        assert.equal(t.journal.target().active,'legacy');
        if(mode==='configuration')assert.equal(await t.io.get(p.COMMIT_KEY),null);
      } finally {t.close();}
    }
  });
  await test('target binding and semantic validator cannot be bypassed by a valid journal hash',async()=>{
    const t=await setup();
    try {
      await assert.rejects(p.run({scope:p.SCOPE,binding:p.hash('other-bucket')},t.journal),/SCOPE/);
      const checked=new p.Journal(path.join(t.root,'journal.sqlite'),()=>{throw Error('SEMANTIC_INVALID');});
      try {await assert.rejects(p.run(t.io,checked),/SEMANTIC_INVALID/);}
      finally {checked.close();}
      await assert.rejects(p.capture(t.io,EXPECTED,OP,SPACE,()=>{throw Error('SEMANTIC_INVALID');}),/SEMANTIC_INVALID/);
      assert.equal(t.io.countTargets(),0);
    } finally {t.close();}
  });
  const boundaries = [
    ...Array.from({length:10},(_,i)=>['object-'+i+'-before','object-'+i+'-after']).flat(),
    'publish-before','publish-after','switch-before','local-before-commit','local-after-commit'
  ];
  for(const boundary of boundaries) await test('process termination and restart: '+boundary,async()=>{
    const t=await setup(),root=t.root;
    t.close();
    await worker(root,boundary); killed++;
    const journal=new p.Journal(path.join(root,'journal.sqlite'));
    try {
      assert.equal(journal.target().active,boundary==='local-after-commit'?SPACE:'legacy');
      assert.equal(journal.load().phase,boundary==='local-after-commit'?'complete':'confirmed');
    } finally {journal.close();}
    await worker(root,'none');
    const resumed=new p.Journal(path.join(root,'journal.sqlite')),remote=new Remote(root);
    try {
      assert.equal(resumed.target().active,SPACE);
      assert.equal(resumed.load().phase,'complete');
      for(const row of resumed.load().plan.entries) assert.equal((await remote.get(row.key)).etag,row.etag);
      assert.equal(remote.countTargets(),11);
    } finally {resumed.close();remote.close();}
  });
  const peer=process.argv.find(v=>v.startsWith('--peer='))?.slice(7);
  if(peer) await test('mirrored prototype and test are byte-identical',()=>{
    for(const name of ['migration-journal-prototype.cjs','test-migration-journal-prototype.cjs'])
      assert.deepEqual(fs.readFileSync(path.join(__dirname,name)),fs.readFileSync(path.join(peer,'scripts',name)));
  });
  for(const root of roots) fs.rmSync(owned(root),{recursive:true});
  console.log('PASS: '+checks+' migration journal checks; '+killed+' terminated child processes resumed. Host SQLite + synthetic object store, not native/S3/power-loss acceptance.');
}
if(process.argv[2]==='--worker') workerMain().catch(error=>{console.error(error);process.exitCode=1;});
else main().catch(error=>{console.error(error);console.error('Failed fixtures retained:',roots);process.exitCode=1;});
