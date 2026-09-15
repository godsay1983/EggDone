const assert = require('node:assert/strict');
module.exports = api => {
  const source = () => ({
    task: {todo_uuid:'source',title:'  Source  ',note:'Keep\r\nnote',updated_at:700,read_only:true,
      items:{format_version:1,items:[
        {uuid:'b',content:'Second',completed:false,sort_order:20,deleted_at:null},
        {uuid:'a',content:'First',completed:true,sort_order:10,deleted_at:null},
        {uuid:'deleted',content:'Hidden',completed:true,sort_order:0,deleted_at:20}
      ]}},
    fields:{due_date:'2026-09-20',due_at:null,reminder_at:1000,group_uuid:'group',priority:1,repeat_rule:'daily'},
    completed:true,repeat_series_uuid:'series',rules:{format_version:1,rules:[{uuid:'rule'}]},
    definitions:{format_version:1,definitions:[{rule_uuid:'rule'}]},next_occurrence_date:'2026-09-21'
  });
  let n = 0;
  const uuid = () => 'new-' + (++n);
  return [
    {name:'copy whitelists content and clears source state with an empty creation baseline',run() {
      const s=source(),before=structuredClone(s),d=api.taskCopyDraft(s,['group'],uuid);
      assert.deepEqual(s,before);assert.equal(d.creation.task.title,'Source');assert.equal(d.creation.task.note,'Keep\nnote');
      assert.deepEqual(d.creation.fields,{due_date:null,due_at:null,reminder_at:null,priority:0,repeat_rule:null,group_uuid:'group'});
      assert.equal(d.creation.task.read_only,false);assert.equal(d.creation.completed,false);assert.equal(d.creation.task.updated_at,0);
      assert.deepEqual(d.creation.task.items,{format_version:1,items:[]});assert.equal(d.creation.repeat_series_uuid,null);
      assert.deepEqual(d.creation.rules.rules,[]);assert.deepEqual(d.creation.definitions.definitions,[]);assert.equal(d.creation.next_occurrence_date,null);
      assert.deepEqual(d.items.map(i=>[i.content,i.completed,i.sort_order]),[['First',false,1000],['Second',false,2000]]);
      assert.equal(d.items.some(i=>s.task.items.items.some(old=>old.uuid===i.uuid)),false);
    }},
    {name:'copies have independent identities and editable data',run() {
      const s=source(),a=api.taskCopyDraft(s,[],uuid),b=api.taskCopyDraft(s,[],uuid);
      assert.notEqual(a.creation.task.todo_uuid,b.creation.task.todo_uuid);assert.notEqual(a.items[0].uuid,b.items[0].uuid);
      a.items[0].content='Edited';a.creation.task.title='Changed';
      assert.equal(b.items[0].content,'First');assert.equal(s.task.title,'  Source  ');
      assert.equal(a.creation.fields.group_uuid,null);
    }},
    {name:'ordinary task and removed group copy without checklist or group',run() {
      const s=source();s.task.items.items=[];s.fields.group_uuid=null;
      const d=api.taskCopyDraft(s,['group'],uuid);assert.equal(d.items.length,0);assert.equal(d.creation.fields.group_uuid,null);
      s.fields.group_uuid='removed';assert.equal(api.taskCopyDraft(s,['group'],uuid).creation.fields.group_uuid,null);
    }},
    {name:'over-limit copied lists fail without truncating source',run() {
      const s=source();s.task.items.items=Array.from({length:21},(_,i)=>({uuid:'i'+i,content:'Item',completed:true,sort_order:i,deleted_at:null}));
      assert.throws(()=>api.taskCopyDraft(s,[],uuid),/TOO_MANY_ITEMS/);assert.equal(s.task.items.items.length,21);
      s.task.items.items[20].deleted_at=99;assert.equal(api.taskCopyDraft(s,[],uuid).items.length,20);
    }},
    {name:'invalid source text is rejected without mutation',run() {
      const s=source();s.task.title=' ';const before=structuredClone(s);
      assert.throws(()=>api.taskCopyDraft(s,[],uuid),/EMPTY_TITLE/);assert.deepEqual(s,before);
    }}
  ];
};
