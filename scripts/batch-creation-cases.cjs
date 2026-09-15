const assert = require('node:assert/strict');
const { randomUUID } = require('node:crypto');
module.exports = ({ BatchCreationSession, batchError }) => {
  const result = r => ({ operation_uuid: r.operation_uuid, task_uuids: r.items.map(i=>i.uuid), created_at:100 });
  const start = (api, text='- milk\n\n- milk\n3. report') => {
    const s=new BatchCreationSession(api,randomUUID);s.input(text);s.review();return s;
  };
  return [

    {name:'reopened session loads exact identities without invoking create',run:async()=>{
      let pending=null,calls=0,lose=true;
      const api={load:async()=>structuredClone(pending),forget:async()=>{pending=null;},
        create:async r=>{calls++;if(pending)assert.deepEqual(r,pending);pending=structuredClone(r);if(lose){lose=false;throw Error('lost');}return result(r);}};
      const a=new BatchCreationSession(api,randomUUID);assert.throws(()=>a.input('blocked'),/LOCKED/);
      await a.initialize();a.input('1. literal prefix\n2. report');a.review();
      await assert.rejects(a.submit([]),/lost/);const original=structuredClone(pending);
      const b=new BatchCreationSession(api,randomUUID);await b.initialize();
      assert.equal(calls,1);assert.equal(b.locked(),true);assert.equal(b.count(),2);
      assert.deepEqual(b.preview.rows.map(r=>r.title),original.items.map(i=>i.title));
      await b.submit([]);assert.equal(calls,2);assert.equal(pending,null);assert.equal(b.done,true);
    }},
    {name:'read failure blocks new tasks and can be retried',run:async()=>{
      let fail=true,calls=0;
      const s=new BatchCreationSession({load:async()=>{if(fail)throw Error('read');return null;},create:async r=>{calls++;return result(r);}},randomUUID);
      await assert.rejects(s.initialize(),/read/);assert.equal(s.ready,false);assert.throws(()=>s.input('a'),/LOCKED/);
      await assert.rejects(s.submit([]),/LOCKED/);assert.equal(calls,0);
      fail=false;await s.initialize();s.input('a');s.review();await s.submit([]);
    }},
    {name:'cleanup failure retries only cleanup after confirmed creation',run:async()=>{
      let calls=0,clears=0;
      const s=start({create:async r=>{calls++;return result(r);},forget:async()=>{if(++clears===1)throw Error('cleanup');}});
      await assert.rejects(s.submit([]),/RECOVERY_CLEANUP/);assert.equal(s.creationConfirmed(),true);assert.equal(s.done,false);assert.equal(s.locked(),true);
      await s.submit([]);assert.equal(calls,1);assert.equal(clears,2);assert.equal(s.done,true);
    }},
    {name:'ending recovery never calls create and failed cleanup preserves request',run:async()=>{
      const req={operation_uuid:randomUUID(),group_uuid:null,items:[{uuid:randomUUID(),title:'1. literal'}]};
      let clears=0;const s=new BatchCreationSession({load:async()=>req,forget:async r=>{assert.deepEqual(r,req);if(++clears===1)throw Error('cleanup');},create:async()=>{throw Error('must not create');}},randomUUID);
      await s.initialize();assert.equal(s.preview.rows[0].title,'1. literal');
      await assert.rejects(s.discard(),/cleanup/);assert.equal(s.locked(),true);assert.equal(s.done,false);
      await s.discard();assert.equal(s.done,true);
    }},
    {name:'failed prewrite cleanup cannot unlock a durable pending request',run:async()=>{
      let clearFails=true;
      const s=start({create:async()=>{throw Error('BATCH_GROUP_MISSING');},forget:async()=>{if(clearFails)throw Error('cleanup');}});
      await assert.rejects(s.submit([]),/cleanup/);assert.equal(s.locked(),true);
      clearFails=false;await assert.rejects(s.submit([]),/GROUP_MISSING/);assert.equal(s.locked(),false);
    }},
    {name:'corrupt recovered payload stays blocked',run:async()=>{
      const s=new BatchCreationSession({load:async()=>({operation_uuid:'bad',items:[]}),create:async()=>{throw Error('unexpected');}},randomUUID);
      await assert.rejects(s.initialize(),/RECOVERY_INVALID/);assert.equal(s.ready,false);
    }},
    {name:'preview is read only; duplicates and line identities survive editing and back',run:async()=>{
      let calls=0;const s=start({create:async r=>{calls++;return result(r);}});
      assert.equal(calls,0);assert.deepEqual(s.preview.rows.map(r=>r.lineNumber),[1,3,4]);
      assert.equal(s.preview.rows[0].duplicate,true);s.change(3,'other',false);
      assert.equal(s.preview.rows[0].duplicate,false);s.back();s.review();assert.equal(s.preview.rows[1].title,'other');
      assert.equal(s.count(),2);s.selectGroup('group');
      await assert.rejects(s.submit([]),/GROUP_MISSING/);assert.equal(calls,0);
      s.selectGroup('');await s.submit([]);assert.equal(calls,1);assert.equal(s.done,true);
    }},
    {name:'invalid rows block only when selected; no truncation or implicit scheduling',run:async()=>{
      const sent=[];const s=start({create:async r=>{sent.push(r);return result(r);}},'- \nTomorrow 15:00 report');
      assert.equal(s.issue(),'EMPTY_TITLE');await assert.rejects(s.submit([]),/EMPTY_TITLE/);assert.equal(sent.length,0);
      s.change(1,'',false);assert.equal(s.issue(),null);await s.submit([]);
      assert.equal(sent[0].items.length,1);assert.equal(sent[0].items[0].title,'Tomorrow 15:00 report');
      assert.deepEqual(Object.keys(sent[0]).sort(),['group_uuid','items','operation_uuid']);
    }},
    {name:'selection, raw limits, and empty preview do not create tasks',run:async()=>{
      let calls=0;const s=new BatchCreationSession({create:async r=>{calls++;return result(r);}},randomUUID);
      for(const [text,code] of [[' ','NO_TASKS_SELECTED'],['x'.repeat(20001),'INPUT_TOO_LONG'],[Array(51).fill('a').join('\n'),'TOO_MANY_TASKS']]){
        s.input(text);assert.throws(()=>s.review(),new RegExp(code));
      }
      s.input(Array(50).fill('a').join('\n'));s.review();s.selectAll(false);await assert.rejects(s.submit([]),/NO_TASKS_SELECTED/);
      s.selectAll(true);assert.equal(s.count(),50);assert.equal(calls,0);
    }},
    {name:'uncertain response locks edits and retries the original immutable request once',run:async()=>{
      const calls=[];let first=true;
      const s=start({create:async r=>{calls.push(structuredClone(r));if(first){first=false;r.items[0].title='mutated adapter';throw Error('lost reply');}return result(r);}});
      await assert.rejects(s.submit([]),/lost reply/);assert.equal(s.locked(),true);
      assert.throws(()=>s.input('new'),/LOCKED/);assert.throws(()=>s.change(1,'new',true),/LOCKED/);
      assert.throws(()=>s.selectGroup('new'),/LOCKED/);assert.throws(()=>s.back(),/LOCKED/);
      await s.submit([]);assert.deepEqual(calls[0],calls[1]);assert.equal(s.done,true);
      await assert.rejects(s.submit([]),/LOCKED/);assert.equal(calls.length,2);
    }},
    {name:'in-flight duplicate submit is rejected',run:async()=>{
      let release;let calls=0;
      const s=start({create:r=>{calls++;return new Promise(resolve=>{release=()=>resolve(result(r));});}});
      const saving=s.submit([]);assert.equal(s.busy,true);await assert.rejects(s.submit([]),/LOCKED/);
      release();await saving;assert.equal(calls,1);assert.equal(s.busy,false);
    }},
    {name:'definite prewrite group rejection unlocks editing',run:async()=>{
      const calls=[];const s=start({create:async r=>{calls.push(r);if(calls.length===1)throw Error('BATCH_GROUP_MISSING');return result(r);}});
      await assert.rejects(s.submit([]),/GROUP_MISSING/);assert.equal(s.locked(),false);
      s.change(1,'fixed',true);await s.submit([]);assert.notEqual(calls[0].operation_uuid,calls[1].operation_uuid);
    }},
    {name:'malformed success and stale receipts never permit new identity retry',run:async()=>{
      const calls=[];const s=start({create:async r=>{calls.push(r);return {...result(r),task_uuids:[]};}});
      await assert.rejects(s.submit([]),/INVALID_RESPONSE/);assert.equal(s.done,false);assert.equal(s.locked(),true);
      await assert.rejects(s.submit([]),/INVALID_RESPONSE/);assert.deepEqual(calls[0],calls[1]);
      assert.equal(batchError('BATCH_STALE_RECEIPT'),'conflict');assert.equal(batchError('offline'),'failed');
    }}
  ];
};
