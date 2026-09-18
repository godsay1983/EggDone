'use strict';
// Reference/prototype tests, not production repositories or native migration.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const p = require('./purge-compat-prototype.cjs');
const SPACE = '123e4567-e89b-42d3-a456-426614174070';
const OTHER = '123e4567-e89b-42d3-a456-426614174071';
const ID = '123e4567-e89b-42d3-a456-426614174000';
const OP = '123e4567-e89b-42d3-a456-426614174080';
const OP2 = '123e4567-e89b-42d3-a456-426614174081';
const row = kind => ({kind, uuid:ID, updated_at:10, deleted_at:10, body:{title:'Private body', completed:true}});
const doc = records => ({...p.empty(SPACE), records});
function memoryIO() {
  const objects = new Map(); let revision = 0;
  return {
    objects,
    async get(key) { return structuredClone(objects.get(key) ?? null); },
    async put(key,text,etag) {
      const old = objects.get(key);
      if (etag === null ? old !== undefined : old?.etag !== etag) return false;
      objects.set(key,{text,etag:'"' + (++revision) + '"'}); return true;
    }
  };
}
let count = 0;
async function test(name, run) {
  try { await run(); count++; } catch(cause) { throw new Error(name,{cause}); }
}
async function main() {
  for (const kind of ['todo','note']) {
    await test(kind + ' terminal dominates every time and restores are content-free', () => {
      const a = row(kind), before = doc([a]);
      const deleted = p.purge(before,a,OP,20);
      assert.equal(deleted.records.length,0);
      assert(!JSON.stringify(deleted).includes('Private body'));
      assert.deepEqual(p.purge(deleted,a,OP,999),deleted);
      for (const time of [0,10,100,Number.MAX_SAFE_INTEGER]) {
        const restored = {...a,updated_at:time,deleted_at:null};
        assert.deepEqual(p.merge(deleted,doc([restored])),deleted);
        assert.deepEqual(p.merge(doc([restored]),deleted),deleted);
        assert.deepEqual(p.restoreBackup(deleted,[restored]),deleted);
      }
      assert.equal(before.records[0].body.title,'Private body');
      assert.equal(p.restoreBackup(p.empty(SPACE),[a]).records.length,1,
        'An independent empty scope cannot know terminal evidence it has never seen');
    });
    await test(kind + ' fixed snapshot rejects restored and same-clock edited rows', () => {
      const a = row(kind);
      assert.throws(()=>p.purge(doc([{...a,deleted_at:null}]),a,OP,20),/PURGE_CONFLICT/);
      assert.throws(()=>p.purge(doc([{...a,body:{title:'Edited'}}]),a,OP,20),/PURGE_CONFLICT/);
      const other = {...a,uuid:OTHER};
      assert.deepEqual(p.purge(doc([a,other]),a,OP,20).records,[other]);
    });
  }
  await test('identity is kind plus UUID; terminal metadata rejects body fields', () => {
    const a = row('todo'), b = row('note');
    const out = p.purge(doc([a,b]),a,OP,20);
    assert.deepEqual(out.records,[b]);
    const bad = structuredClone(out); bad.terminals[0].title = 'leaked';
    assert.throws(()=>p.validate(bad,SPACE),/INVALID_SHAPE/);
    assert.throws(()=>p.validate({...out,space_id:OTHER},SPACE),/SCOPE/);
    assert.throws(()=>p.validate(doc([a,a]),SPACE),/DUPLICATE/);
    assert.throws(()=>p.validate(doc([null]),SPACE),/INVALID_SHAPE/);
    assert.throws(()=>p.validate(doc([{...a,updated_at:NaN}]),SPACE),/INVALID_VERSION/);
    assert.throws(()=>p.objectKey('../todos.json'),/INVALID_ID/);
    assert.throws(()=>p.parse('x'.repeat(p.MAX_BYTES+1),SPACE),/TOO_LARGE/);
    assert.throws(()=>p.validate(doc(Array(p.MAX_ROWS+1).fill(a)),SPACE),/LIMIT/);
    assert.throws(()=>p.parse('{}',SPACE),/INVALID_SHAPE/);
  });
  await test('125 merge association combinations, commutative and idempotent', () => {
    const a = row('todo');
    const sets = [p.empty(SPACE),doc([a]),doc([{...a,updated_at:999,deleted_at:null}]),
      p.purge(doc([a]),a,OP,20),p.purge(doc([a]),a,OP2,1)];
    for (const x of sets) {
      assert.deepEqual(p.merge(x,x),x);
      for (const y of sets) {
        assert.deepEqual(p.merge(x,y),p.merge(y,x));
        for (const z of sets) assert.deepEqual(p.merge(p.merge(x,y),z),p.merge(x,p.merge(y,z)));
      }
    }
  });
  await test('sync rejects missing/corrupt/foreign roots, stale ETag and old-scope ACK', async () => {
    const io=memoryIO(), d=doc([row('todo')]), key=p.objectKey(SPACE);
    await assert.rejects(p.sync(io,d),/REMOTE_MISSING/);
    await io.put(key,p.canonical(d),null);
    const racing={get:io.get,put:async(k,text,etag)=>{
      await io.put(k,p.canonical(p.empty(SPACE)),etag);
      return io.put(k,text,etag);
    }};
    await assert.rejects(p.sync(racing,d),/CAS_CONFLICT/);
    let active=true;
    const changing={get:io.get,put:async(...args)=>{const ok=await io.put(...args);active=false;return ok;}};
    await assert.rejects(p.sync(changing,d,()=>{if(!active)throw Error('CONFIG_CHANGED');}),/CONFIG_CHANGED/);
    const bad={get:async()=>({text:'{}',etag:'"etag"'}),put:async()=>assert.fail('must not write')};
    await assert.rejects(p.sync(bad,d),/INVALID_SHAPE/);
    bad.get=async()=>({text:p.canonical(p.empty(OTHER)),etag:'"etag"'});
    await assert.rejects(p.sync(bad,d),/SCOPE_MISMATCH/);
    bad.get=async()=>({text:p.canonical(d),etag:null});
    await assert.rejects(p.sync(bad,d),/ETAG_REQUIRED/);
  });
  await test('two offline new peers converge after conflicting restore/purge and lost reply',async()=>{
    const io=memoryIO(), a=row('todo'), key=p.objectKey(SPACE);
    await io.put(key,p.canonical(doc([a])),null);
    const deleted=p.purge(doc([a]),a,OP,20);
    const restored=doc([{...a,deleted_at:null,updated_at:Number.MAX_SAFE_INTEGER}]);
    let drop=true;
    const lost={get:io.get,put:async(...args)=>{
      const ok=await io.put(...args); if(drop){drop=false;throw Error('REPLY_LOST');} return ok;
    }};
    await assert.rejects(p.sync(lost,deleted),/REPLY_LOST/);
    assert.deepEqual(await p.sync(io,restored),deleted);
    assert.deepEqual(await p.sync(io,deleted),deleted);
  });
  const seed=doc([row('todo')]);
  async function setup() {
    const io=memoryIO(); await io.put('account/todos.json','old snapshot',null);
    const source=await io.get('account/todos.json');
    const plan=p.migration('account/todos.json',source.etag,SPACE,OP,seed);
    return {io,plan,source};
  }
  await test('migration requires confirmation and unchanged reviewed source',async()=>{
    const {io,plan,source}=await setup();
    await assert.rejects(p.publish(io,plan,false),/CONFIRM_REQUIRED/);
    assert.equal(io.objects.size,1);
    await io.put(plan.sourceKey,'late update',source.etag);
    await assert.rejects(p.publish(io,plan,true),/SOURCE_CHANGED/);
    assert.equal(io.objects.size,1);
  });
  // Interrupt before and after each of the initial migration's seven I/O boundaries.
  for (const when of ['before','after']) for(let at=1;at<=7;at++) {
    await test('migration interrupted '+when+' I/O '+at,async()=>{
      const {io,plan}=await setup(); let step=0, interrupted=false;
      const flaky={};
      for(const method of ['get','put']) flaky[method]=async(...args)=>{
        const current=++step;
        if(current===at && when==='before'){interrupted=true;throw Error('INTERRUPTED');}
        const result=await io[method](...args);
        if(current===at && when==='after'){interrupted=true;throw Error('INTERRUPTED');}
        return result;
      };
      let installed=null;
      try { installed=await p.publish(flaky,plan,true); } catch(e){assert.match(e.message,/INTERRUPTED/);}
      assert(interrupted,'selected boundary must execute');
      assert.equal(installed,null,'caller must not switch target before success');
      const resumed=await p.publish(io,plan,true);
      assert.equal(resumed.space_id,SPACE);
      assert.equal((await io.get(plan.sourceKey)).text,'old snapshot');
      assert.equal((await io.get(p.objectKey(SPACE))).text,plan.seed);
      const once=structuredClone([...io.objects]);
      assert.deepEqual(await p.publish(io,plan,true),resumed);
      assert.deepEqual([...io.objects],once,'idempotent resume must not reset active checkpoint');
    });
  }
  await test('two migrations elect one commit without deleting the old space',async()=>{
    const {io,plan}=await setup();
    const second=p.migration(plan.sourceKey,plan.sourceEtag,OTHER,OP2,p.empty(OTHER));
    const results=await Promise.allSettled([p.publish(io,plan,true),p.publish(io,second,true)]);
    assert.equal(results.filter(r=>r.status==='fulfilled').length,1);
    assert.equal(results.filter(r=>r.status==='rejected').length,1);
    assert.equal((await io.get(plan.sourceKey)).text,'old snapshot');
  });
  await test('old writes after switch remain separate, not silently merged into new state',async()=>{
    const {io,plan}=await setup();
    await p.publish(io,plan,true);
    const source=await io.get(plan.sourceKey);
    await io.put(plan.sourceKey,'late old update',source.etag);
    assert.equal((await io.get(p.objectKey(SPACE))).text,plan.seed);
    assert.equal((await io.get(plan.sourceKey)).text,'late old update');
    const advanced=p.purge(seed,row('todo'),OP2,25);
    await p.sync(io,advanced);
    await p.publish(io,plan,true);
    assert.deepEqual(p.parse((await io.get(p.objectKey(SPACE))).text,SPACE),advanced);
  });
  await test('migration source changes after staging and corrupt stages never activate',async()=>{
    const {io,plan,source}=await setup();
    const racing={get:io.get,put:async(key,text,etag)=>{
      const ok=await io.put(key,text,etag);
      if(key===p.objectKey(SPACE)) await io.put(plan.sourceKey,'late source edit',source.etag);
      return ok;
    }};
    await assert.rejects(p.publish(racing,plan,true),/SOURCE_CHANGED/);
    assert.equal(await io.get('account/lifecycle-prototype/v1/migration.json'),null);
    const fresh=await setup();
    await fresh.io.put(p.objectKey(SPACE),'corrupt staging bytes',null);
    await assert.rejects(p.publish(fresh.io,fresh.plan,true),/STAGE_CONFLICT/);
    assert.equal(await fresh.io.get('account/lifecycle-prototype/v1/migration.json'),null);
  });
  await test('late old write after source recheck is split, not atomic migration of legacy data',async()=>{
    const {io,plan,source}=await setup();
    const racing={get:io.get,put:async(key,text,etag)=>{
      if(key.endsWith('/migration.json')) await io.put(plan.sourceKey,'unobserved late write',source.etag);
      return io.put(key,text,etag);
    }};
    await p.publish(racing,plan,true);
    assert.equal((await io.get(plan.sourceKey)).text,'unobserved late write');
    assert.equal((await io.get(p.objectKey(SPACE))).text,plan.seed);
    // This is a documented limitation: a legacy writer cannot join a multi-object freeze.
  });
  await test('migration foreign target, staged conflict and config changes fail closed',async()=>{
    const {io,plan}=await setup();
    let checks=0;
    await assert.rejects(p.publish(io,plan,true,()=>{if(++checks===3)throw Error('CONFIG_CHANGED');}),/CONFIG_CHANGED/);
    assert.equal(await io.get('account/lifecycle-prototype/v1/migration.json'),null);
    assert.throws(()=>p.migration('other/todos.json',plan.sourceEtag,SPACE,OP,seed),/MIGRATION_SOURCE/);
    assert.throws(()=>p.migration(plan.sourceKey,null,SPACE,OP,seed),/MIGRATION_SOURCE/);
    assert.throws(()=>p.migration(plan.sourceKey,plan.sourceEtag,OTHER,OP,seed),/SCOPE/);
  });
  const peer=process.argv.find(x=>x.startsWith('--peer='))?.slice(7);
  if(peer) await test('prototype artifacts match peer byte-for-byte',()=>{
    for(const name of ['scripts/purge-compat-prototype.cjs','scripts/test-purge-compat-prototype.cjs']) {
      assert.deepEqual(fs.readFileSync(path.join(__dirname,'..',name)),fs.readFileSync(path.join(peer,name)),name);
    }
  });
  console.log('PASS: '+count+' purge prototype checks; 125 merge associations. Not production/native migration acceptance.');
}
main().catch(e=>{console.error(e);process.exitCode=1;});
