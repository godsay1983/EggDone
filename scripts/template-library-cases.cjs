const assert = require('node:assert/strict');
module.exports = api => {
  let n=0;const uuid=()=> '00000000-0000-4000-8000-'+String(++n).padStart(12,'0');
  const content=()=>({name:'Travel',title:'Pack',note:'Notes',group_uuid:null,checklist:['Ticket','Passport']});
  const row=()=>({uuid:uuid(),content:content(),created_at:1,updated_at:2,updated_by:'device',deleted_at:null});
  return [
    {name:'default template name never splits a surrogate pair at the name limit',run(){
      const c=api.templateFromTask({task:{title:'a'.repeat(59)+'\uD83D\uDE00',note:'',items:{items:[]}},fields:{group_uuid:null}});
      assert.equal(c.name,'a'.repeat(59));assert.equal(c.title.length,61);
    }},
    {name:'template whitelist and task creation clear inherited state and allocate new IDs',run(){
      const source={task:{title:'Pack',note:'Notes',items:{items:[
        {uuid:'z',content:'Passport',completed:true,sort_order:2,deleted_at:null},
        {uuid:'a',content:'Ticket',completed:false,sort_order:1,deleted_at:null},
        {uuid:'d',content:'Deleted',completed:false,sort_order:0,deleted_at:1}]}},fields:{group_uuid:null,reminder_at:1000,priority:1}};
      const c=api.templateFromTask(source);assert.deepEqual(c,{...content(),name:'Pack'});
    }},
    {name:'template use is independent and retains no scheduled or completed state',run(){
      const c=content(),a=api.templateCreation(c,[],uuid),b=api.templateCreation(c,[],uuid);
      assert.notEqual(a.creation.task.todo_uuid,b.creation.task.todo_uuid);
      assert.notEqual(a.items[0].uuid,b.items[0].uuid);
      assert.deepEqual(a.creation.task.items,{format_version:1,items:[]});assert.equal(a.creation.task.updated_at,0);
      assert.deepEqual(a.creation.fields,{due_date:null,due_at:null,reminder_at:null,group_uuid:null,priority:0,repeat_rule:null});
      assert.equal(a.creation.completed,false);assert.equal(a.creation.repeat_series_uuid,null);
      assert.ok(a.items.every(i=>!i.completed));a.items[0].content='changed';assert.equal(b.items[0].content,'Ticket');
      assert.deepEqual(c,content());
    }},
    {name:'missing groups and oversized or empty-content drafts require explicit correction',run(){
      assert.throws(()=>api.templateCreation({...content(),group_uuid:uuid()},[],uuid),/GROUP_MISSING/);
      assert.throws(()=>api.templateCreation({...content(),checklist:Array(21).fill('x')},[],uuid),/TOO_MANY/);
      assert.throws(()=>api.templateCreation({...content(),title:''},[],uuid),/EMPTY_TITLE/);
      assert.throws(()=>api.templateCreation({...content(),checklist:['']},[],uuid),/EMPTY_ITEM/);
      assert.equal(api.templateCreation({...content(),checklist:[]},[],uuid).items.length,0);
    }},
    {name:'listing filters tombstones and sorts without writes; selecting clones baseline',async run(){
      const a=row(),b=row();a.updated_at=9;let writes=0,request;
      const session=new api.TemplateLibrarySession({list:async()=>({templates:[b,{...row(),deleted_at:3},a]}),
        save:async r=>{writes++;request=r;return {...r.expected,content:r.content,updated_at:10};}},uuid);
      assert.deepEqual(await session.list(),[a,b]);session.select(a);a.updated_at=22;
      assert.equal(writes,0);await session.save(content(),false);assert.equal(request.expected.updated_at,9);
    }},
    {name:'uncertain create retries exact request; changed content retains identity not duplicate template',async run(){
      const calls=[];let fail=true;
      const session=new api.TemplateLibrarySession({list:async()=>({templates:[]}),save:async r=>{
        calls.push(structuredClone(r));if(fail)throw Error('offline');return {...row(),uuid:r.uuid,content:r.content};
      }},uuid);session.select(null);
      await assert.rejects(session.save(content(),false));await assert.rejects(session.save(content(),false));
      assert.deepEqual(calls[0],calls[1]);await assert.rejects(session.save({...content(),name:'Different'},false));
      assert.equal(calls[2].uuid,calls[0].uuid);assert.notEqual(calls[2].operation_uuid,calls[0].operation_uuid);
      fail=false;await session.save({...content(),name:'Different'},false);assert.deepEqual(calls[2],calls[3]);
    }},
    {name:'busy save prevents concurrent writes or changing selection; result becomes next baseline',async run(){
      let finish,request;
      const session=new api.TemplateLibrarySession({list:async()=>({templates:[]}),save:r=>{request=r;return new Promise(resolve=>finish=resolve);}},uuid);
      const r=row();session.select(r);const saving=session.save(content(),false);
      await assert.rejects(session.save(content(),false),/BUSY/);assert.throws(()=>session.select(null),/BUSY/);
      finish({...r,updated_at:3});await saving;
      const deleting=session.save(content(),true);assert.equal(request.expected.updated_at,3);assert.equal(request.deleted,true);
      finish({...r,updated_at:4,deleted_at:4});await deleting;
    }},
    {name:'conflicts preserve baseline and stable retry until explicit reload selection',async run(){
      const calls=[];const r=row();
      const session=new api.TemplateLibrarySession({list:async()=>({templates:[]}),save:async request=>{
        calls.push(request);throw Error('TEMPLATE_STALE_DRAFT');
      }},uuid);session.select(r);
      await assert.rejects(session.save(content(),false),/STALE/);await assert.rejects(session.save(content(),false),/STALE/);
      assert.deepEqual(calls[0],calls[1]);session.select({...r,updated_at:10});
      await assert.rejects(session.save(content(),false));assert.equal(calls[2].expected.updated_at,10);
      assert.equal(api.templateError('TEMPLATE_STALE_DRAFT'),'conflict');
    }}
  ];
};
