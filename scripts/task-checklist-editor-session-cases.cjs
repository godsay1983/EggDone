const assert = require('node:assert/strict');
const clone = value => JSON.parse(JSON.stringify(value));
const snapshot = () => ({
  task: {todo_uuid:'todo',title:'original',note:'body',updated_at:10,read_only:false,items:{format_version:1,items:[]}},
  fields: {due_date:'2026-09-20',due_at:null,reminder_at:5000,group_uuid:null,priority:0,repeat_rule:null},
  completed:false,repeat_series_uuid:null,rules:{format_version:1,rules:[]},definitions:{format_version:1,definitions:[]}
});
const draft = () => ({title:' edited ',note:'body',items:[{uuid:'item',content:'step',completed:false,sort_order:0}],
  fields:snapshot().fields,mode:'keep',replaces_uuid:null,replacement:null,future_entries:[]});
module.exports = Session => {
  function setup() {
    const e = {calls:[],source:snapshot(),saveResult:{updated_at:20,rule_uuid:null,reminder_changed:false}};
    e.read = async () => clone(e.source);
    e.write = async () => e.saveResult;
    let id=0;
    e.session = new Session({read:id=>e.read(id),save:async r=>{e.calls.push(clone(r));return e.write(r);}},()=>String(++id));
    return e;
  }
  return [
    {name:'read and abandoned draft never write; all baselines are isolated',run:async()=>{
      const e=setup(),s=await e.session.load('todo');s.task.updated_at=999;s.rules.rules.push({uuid:'fake'});s.task.items.items.push({uuid:'fake'});
      assert.equal(e.calls.length,0);await e.session.save(draft());
      assert.equal(e.calls[0].task.expected_updated_at,10);assert.deepEqual(e.calls[0].expected_rules,snapshot().rules);
      assert.deepEqual(e.calls[0].task.expected_items,snapshot().task.items);assert.equal(e.calls[0].task.title,'edited');
    }},
    {name:'uncertain save retries the same request; success requires reload',run:async()=>{
      const e=setup();await e.session.load('todo');e.write=async()=>{throw Error('timeout');};
      await assert.rejects(e.session.save(draft()),/timeout/);e.write=async()=>e.saveResult;
      assert.deepEqual(await e.session.save(draft()),e.saveResult);assert.deepEqual(e.calls[0],e.calls[1]);
      await assert.rejects(e.session.save({...draft(),note:'changed'}),/RELOAD_REQUIRED/);assert.equal(e.calls.length,2);
      await e.session.load('todo');await e.session.save(draft());assert.notEqual(e.calls[2].task.operation_uuid,e.calls[1].task.operation_uuid);
    }},
    {name:'changed fields and future definition receive new operation identities',run:async()=>{
      const e=setup();await e.session.load('todo');e.write=async()=>{throw Error('write');};
      const d=draft();await assert.rejects(e.session.save(d),/write/);
      d.fields.priority=1;await assert.rejects(e.session.save(d),/write/);
      d.mode='replace';d.replacement={uuid:'rule',schedule:{anchor_date:'2026-09-20'},timezone_id:null};
      d.future_entries=[{uuid:'entry',content:'future',sort_order:0}];
      await assert.rejects(e.session.save(d),/write/);
      assert.equal(new Set(e.calls.map(r=>r.task.operation_uuid)).size,3);
      assert.equal(e.calls[2].future_entries[0].content,'future');assert.equal(e.calls[2].task.expected_updated_at,10);
    }},
    {name:'save is single flight and request is frozen before caller or gateway mutation',run:async()=>{
      const e=setup();await e.session.load('todo');let fail;e.write=r=>{r.fields.priority=1;return new Promise((_,reject)=>fail=reject);};
      const d=draft(),pending=e.session.save(d);d.items[0].content='changed';
      await assert.rejects(e.session.save(d),/BUSY/);await assert.rejects(e.session.load('todo'),/BUSY/);
      fail(Error('retry'));await assert.rejects(pending,/retry/);e.write=async()=>e.saveResult;
      await e.session.save(draft());assert.deepEqual(e.calls[0],e.calls[1]);assert.equal(e.calls[0].task.items[0].content,'step');
    }},
    {name:'failed reload invalidates an earlier baseline',run:async()=>{
      const e=setup();await e.session.load('todo');e.read=async()=>{throw Error('read');};
      await assert.rejects(e.session.load('todo'),/read/);await assert.rejects(e.session.save(draft()),/NOT_LOADED/);assert.equal(e.calls.length,0);
    }},
    {name:'out-of-order load never restores the older task baseline',run:async()=>{
      const e=setup();let first;e.read=()=>new Promise(resolve=>first=resolve);const pending=e.session.load('todo');
      e.source.task.todo_uuid='second';e.read=async()=>clone(e.source);await e.session.load('second');
      first(snapshot());await assert.rejects(pending,/SUPERSEDED/);await e.session.save(draft());assert.equal(e.calls[0].task.todo_uuid,'second');
    }},
    {name:'latest failed load cannot be replaced by a late earlier success',run:async()=>{
      const e=setup();let first;e.read=()=>new Promise(resolve=>first=resolve);const pending=e.session.load('todo');
      e.read=async()=>{throw Error('read');};await assert.rejects(e.session.load('second'),/read/);
      first(snapshot());await assert.rejects(pending,/SUPERSEDED/);await assert.rejects(e.session.save(draft()),/NOT_LOADED/);
    }},
    {name:'archived task cannot save; completed task cannot change future rules',run:async()=>{
      const e=setup();e.source.task.read_only=true;await e.session.load('todo');await assert.rejects(e.session.save(draft()),/READ_ONLY/);
      e.source.task.read_only=false;e.source.completed=true;await e.session.load('todo');
      await assert.rejects(e.session.save({...draft(),mode:'stop'}),/RULE_CONFLICT/);await e.session.save(draft());assert.equal(e.calls.length,1);
    }},
    {name:'unexpected parent identity cannot become a baseline',run:async()=>{
      const e=setup();e.source.task.todo_uuid='other';await assert.rejects(e.session.load('todo'),/MISMATCH/);
      await assert.rejects(e.session.save(draft()),/NOT_LOADED/);assert.equal(e.calls.length,0);
    }},
    {name:'result passes reminder reconciliation data without changing the visible draft',run:async()=>{
      const e=setup();e.saveResult={updated_at:30,rule_uuid:'new-rule',reminder_changed:true};await e.session.load('todo');
      const d=draft(),before=clone(d);assert.deepEqual(await e.session.save(d),e.saveResult);assert.deepEqual(d,before);
    }}
  ];
};
