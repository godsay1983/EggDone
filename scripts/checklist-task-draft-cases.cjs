const assert=require('node:assert/strict');
const clone=x=>JSON.parse(JSON.stringify(x));
module.exports=api=>{
 function snapshot(){return {task:{todo_uuid:'todo',title:'task',note:'note',updated_at:1,read_only:false,items:{format_version:1,items:[]}},
 fields:{due_date:'2026-09-20',due_at:null,reminder_at:50,group_uuid:null,priority:0,repeat_rule:null},
 completed:false,repeat_series_uuid:null,rules:{format_version:1,rules:[]},definitions:{format_version:1,definitions:[]},next_occurrence_date:'2026-09-21'};}
 const items=[{uuid:'item',content:'step',sort_order:1000,completed:true}];
 function builder(){let n=0;return new api.ChecklistTaskDraft(()=>String(++n));}
 const resolve=async(s,z)=>s.local_time_minutes===null?null:Date.parse(s.anchor_date+'T00:00:00Z')+s.local_time_minutes*60000;
 function build(b,s,fields=s.fields,choice='keep',form=null,future=false,resolver=resolve){return b.build(s,'task','note',items,fields,choice,form,future,'UTC',resolver);}
 return [
 {name:'all task fields and checklist share one draft without mutating baseline',run:async()=>{
 const s=snapshot(),before=clone(s),fields={...s.fields,due_date:null,due_at:5000,reminder_at:null,priority:1,group_uuid:'work'};
 const d=await build(builder(),s,fields);assert.deepEqual(d.fields,fields);assert.deepEqual(d.items,items);assert.deepEqual(s,before);assert.equal(d.mode,'keep');
 }},
 {name:'opening a legacy task does not convert recurrence',run:async()=>{
 const s=snapshot();s.fields.repeat_rule='daily';s.repeat_series_uuid='root';
 assert.equal((await build(builder(),s)).mode,'keep');assert.equal(api.checklistLegacyFuture(s),true);
 }},
 {name:'legacy future converts weekdays with a valid next weekday and unchecked definition',run:async()=>{
 const s=snapshot();s.fields.repeat_rule='weekdays';s.repeat_series_uuid='root';
 const b=builder(),d=await build(b,s,s.fields,'keep',null,true);
 assert.equal(d.mode,'replace');assert.equal(d.fields.repeat_rule,null);assert.equal(d.replaces_uuid,null);
 assert.equal(d.fields.due_date,'2026-09-21');assert.deepEqual(d.replacement.schedule.weekdays,[1,2,3,4,5]);
 assert.equal(d.items[0].completed,true);assert.equal('completed' in d.future_entries[0],false);
 assert.deepEqual(await build(b,s,s.fields,'keep',null,true),d);
 }},
 {name:'new recurring task keeps reminder and uses date resolver for timed schedules',run:async()=>{
 const s=snapshot(),f={...s.fields,due_date:null,due_at:Date.parse('2026-09-20T15:30:00Z')};let called=0;
 const d=await build(builder(),s,f,'daily',null,false,async(s,z)=>{called++;assert.equal(z,'UTC');return 123456;});
 assert.equal(called,1);assert.equal(d.fields.due_at,123456);assert.equal(d.fields.due_date,null);assert.equal(d.fields.reminder_at,50);
 }},
 {name:'new repeat requires a date and custom requires confirmed configuration',run:async()=>{
 const s=snapshot();await assert.rejects(build(builder(),s,{...s.fields,due_date:null},'daily'),/DATE_REQUIRED/);
 await assert.rejects(build(builder(),s,s.fields,'custom'),/CONFIGURE_RULE/);
 }},
 {name:'new past reminder is rejected while original past reminder remains legal',run:async()=>{
 const s=snapshot();await build(builder(),s);await assert.rejects(build(builder(),s,{...s.fields,reminder_at:1}),/REMINDER_PAST/);
 }},
 {name:'completed and historical repeat contexts cannot create or replace recurrence',run:async()=>{
 const s=snapshot();s.completed=true;await assert.rejects(build(builder(),s,s.fields,'daily'),/RULE_CONFLICT/);
 s.completed=false;s.repeat_series_uuid='history';assert.equal(api.checklistCanChangeRepeat(s),false);
 }},
 {name:'custom configuration starts with remaining count not original total',run:async()=>{
 const s=snapshot();s.repeat_series_uuid='root';s.rules.rules=[{uuid:'old',first_todo_uuid:'root',current_todo_uuid:'todo',
 deleted_at:null,exhausted:false,current_date:'2026-09-20',generated_count:3,timezone_id:null,
 schedule:{anchor_date:'2026-09-18',frequency:'daily',interval:1,weekdays:[],month_day:null,end_type:'count',end_date:null,max_occurrences:5,local_time_minutes:null}}];
 const form=api.checklistRuleForm(s,s.fields,'custom','UTC');assert.equal(form.count,'3');
 const b=builder(),d=await build(b,s,s.fields,'custom',form);assert.equal(d.replaces_uuid,'old');assert.equal(d.replacement.schedule.max_occurrences,3);
 const stop=await build(b,s,s.fields,'none');assert.equal(stop.mode,'stop');assert.equal(stop.replaces_uuid,'old');
 assert.deepEqual(await build(b,s,s.fields,'custom',form),d);
 }},
 {name:'legacy stop is atomic and retains the current checklist',run:async()=>{
 const s=snapshot();s.fields.repeat_rule='daily';const d=await build(builder(),s,s.fields,'none');
 assert.equal(d.mode,'stop');assert.equal(d.fields.repeat_rule,null);assert.deepEqual(d.items,items);assert.deepEqual(d.future_entries,[]);
 }},
 {name:'resolver rejection creates no incomplete request; retry is deterministic',run:async()=>{
 const s=snapshot(),b=builder();await assert.rejects(build(b,s,s.fields,'daily',null,false,async()=>{throw Error('zone');}),/zone/);
 const d=await build(b,s,s.fields,'daily');assert.deepEqual(await build(b,s,s.fields,'daily'),d);
 }}
 ];
};
