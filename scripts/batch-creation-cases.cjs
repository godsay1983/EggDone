const assert = require('node:assert/strict');
const { randomUUID } = require('node:crypto');
module.exports = ({ BatchCreationSession, batchError }) => {
  const result = r => ({ operation_uuid: r.operation_uuid, task_uuids: r.items.map(i=>i.uuid), created_at:100 });
  const start = (api, text='- milk\n\n- milk\n3. report') => {
    const s=new BatchCreationSession(api,randomUUID);s.input(text);s.review();return s;
  };
  return [
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
