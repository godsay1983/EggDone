const assert=require('node:assert/strict');
const clone=x=>JSON.parse(JSON.stringify(x));
function snapshot(){
 const rule={uuid:'rule',first_todo_uuid:'root',current_todo_uuid:'todo',current_date:'2026-09-20',generated_count:3,
  exhausted:false,deleted_at:null,timezone_id:null,schedule:{anchor_date:'2026-09-18',frequency:'daily',interval:1,weekdays:[],
   month_day:null,end_type:'count',end_date:null,max_occurrences:5,local_time_minutes:null}};
 return {task:{todo_uuid:'todo',title:'task',note:'note',updated_at:10,read_only:false,items:{format_version:1,items:[]}},
 fields:{due_date:'2026-09-20',due_at:null,reminder_at:5000,priority:0,group_uuid:null,repeat_rule:null},completed:false,
 repeat_series_uuid:'root',rules:{format_version:1,rules:[rule]},definitions:{format_version:1,definitions:[]},next_occurrence_date:'2026-09-21'};
}
const items=()=>[{uuid:'item',content:'step',completed:true,sort_order:1000}];
module.exports=api=>{
 function builder(){let n=0;return new api.ChecklistScopeDraft(()=>String(++n));}
 return [
 {name:'current scope preserves all fields and never creates a future definition',run:()=>{
  const s=snapshot(),b=builder(),d=b.build(s,'new','note',items(),false);
  assert.equal(d.mode,'keep');assert.deepEqual(d.fields,s.fields);assert.equal(d.replacement,null);assert.deepEqual(d.future_entries,[]);
 }},
 {name:'future scope retains only remaining occurrences including current task',run:()=>{
  const s=snapshot(),before=clone(s),d=builder().build(s,'task','note',items(),true);
  assert.equal(d.mode,'replace');assert.equal(d.replaces_uuid,'rule');assert.equal(d.replacement.schedule.max_occurrences,3);
  assert.equal(d.replacement.schedule.anchor_date,'2026-09-20');assert.deepEqual(d.fields,s.fields);assert.deepEqual(s,before);
  assert.equal(d.items[0].completed,true);assert.equal('completed' in d.future_entries[0],false);assert.notEqual(d.future_entries[0].uuid,'item');
 }},
 {name:'failed retries and returning to future preserve rule and entry identities',run:()=>{
  const s=snapshot(),b=builder(),first=b.build(s,'task','note',items(),true);b.build(s,'task','note',items(),false);
  assert.deepEqual(b.build(s,'task','note',items(),true),first);
  const d=b.build(s,'task','note',[{...items()[0],content:'changed'},{...items()[0],uuid:'second'}],true);
  assert.equal(d.replacement.uuid,first.replacement.uuid);assert.equal(d.future_entries[0].uuid,first.future_entries[0].uuid);
  assert.notEqual(d.future_entries[0].uuid,d.future_entries[1].uuid);
 }},
 {name:'clearing future checklist creates an explicit empty definition',run:()=>{
  const d=builder().build(snapshot(),'task','note',[],true);assert.equal(d.mode,'replace');assert.deepEqual(d.future_entries,[]);
 }},
 {name:'readonly completed stopped historical mismatched and legacy contexts reject future',run:()=>{
  const mutations=[s=>s.task.read_only=true,s=>s.completed=true,s=>s.rules.rules[0].deleted_at=1,
   s=>s.rules.rules[0].exhausted=true,s=>s.rules.rules[0].current_todo_uuid='other',s=>s.repeat_series_uuid='other',
   s=>s.repeat_series_uuid=null,s=>s.fields.repeat_rule='daily',s=>s.fields.due_date='2026-09-21',
   s=>s.rules.rules.push(clone(s.rules.rules[0])),s=>s.rules.rules=[],s=>s.next_occurrence_date=null];
  for(const mutate of mutations){const s=snapshot();mutate(s);assert.equal(api.checklistFutureRule(s),null);
   assert.throws(()=>builder().build(s,'task','note',items(),true),/UNAVAILABLE/);}
 }},
 {name:'last occurrence or end date disallows a misleading future option',run:()=>{
  const s=snapshot();s.rules.rules[0].schedule.max_occurrences=3;assert.equal(api.checklistFutureRule(s),null);
  s.rules.rules[0].schedule.max_occurrences=null;s.rules.rules[0].schedule.end_date='2026-09-20';assert.equal(api.checklistFutureRule(s),null);
 }},
 {name:'weekly monthly timezone and end settings remain unchanged except anchor and remaining count',run:()=>{
  for(const frequency of ['weekly','monthly']){const s=snapshot(),r=s.rules.rules[0];r.schedule.frequency=frequency;
   r.schedule.interval=2;r.schedule.weekdays=frequency==='weekly'?[1,3]:[];r.schedule.month_day=frequency==='monthly'?0:null;
   r.schedule.local_time_minutes=900;r.timezone_id='Asia/Shanghai';s.fields.due_date=null;s.fields.due_at=123456;
   const d=builder().build(s,'task','note',items(),true),expected=clone(r.schedule);expected.anchor_date=r.current_date;expected.max_occurrences=3;
   assert.deepEqual(d.replacement.schedule,expected);assert.equal(d.replacement.timezone_id,r.timezone_id);assert.deepEqual(d.fields,s.fields);
  }
 }},
 {name:'imported oversize current checklist is preserved but cannot become a future definition',run:()=>{
  const many=Array.from({length:21},(_,n)=>({...items()[0],uuid:String(n)})),b=builder();
  assert.equal(b.build(snapshot(),'task','note',many,false).items.length,21);
  assert.throws(()=>b.build(snapshot(),'task','note',many,true),/UNAVAILABLE/);
 }}
 ];
};
