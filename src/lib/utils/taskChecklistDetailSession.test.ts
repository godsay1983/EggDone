import { describe, it, expect } from 'vitest';
import { TaskChecklistDetailSession } from './taskChecklistDetailSession';
import type { ChecklistItem, ChecklistPanelSnapshot, ChecklistSave } from '../types/taskChecklist';

function setup() {
  const item = (uuid: string): ChecklistItem => ({uuid,todo_uuid:'task',content:uuid,sort_order:1000,completed:false,
    source_rule_uuid:null,source_entry_uuid:null,created_at:1,updated_at:1,updated_by:'device',deleted_at:null});
  const state: ChecklistPanelSnapshot = {todo_uuid:'task',title:'Persisted',note:'Keep',updated_at:1,read_only:false,
    items:{format_version:1,items:[item('a'),item('b'),{...item('deleted'),deleted_at:2}]}};
  const calls: ChecklistSave[] = [];
  let fail = false, readsFail = false, lost = false;
  const api = {read:async()=>{if(readsFail)throw Error('read');return structuredClone(state);},save:async(r:ChecklistSave)=>{
    calls.push(structuredClone(r));if(fail)throw Error('write');
    state.items.items=state.items.items.map(i=>({...i,completed:r.items.find(e=>e.uuid===i.uuid)?.completed??i.completed}));state.updated_at++;
    if(lost){lost=false;throw Error('reply lost');}return state.updated_at;
  }};
  const session=new TaskChecklistDetailSession(api,()=>crypto.randomUUID());
  return {state,calls,session,api,fail:()=>{fail=true;},recover:()=>{fail=false;},failRead:()=>{readsFail=true;},lose:()=>{lost=true;}};
}
describe('persisted checklist detail session',()=>{
  it('only changes the selected persisted item and retains tombstones in the expected snapshot',async()=>{
    const e=setup();const shown=await e.session.load('task');shown.title='Unsaved';shown.items.items[1].content='Draft';
    await e.session.setCompleted('a',true);const r=e.calls[0];
    expect(r.title).toBe('Persisted');expect(r.note).toBe('Keep');expect(r.expected_items.items).toHaveLength(3);
    expect(r.items).toEqual([{uuid:'a',content:'a',sort_order:1000,completed:true},{uuid:'b',content:'b',sort_order:1000,completed:false}]);
    await expect(e.session.setCompleted('b',true)).rejects.toThrow('RELOAD_REQUIRED');
    await e.session.load('task');await e.session.setCompleted('b',true);expect(e.calls[1].items.every(i=>i.completed)).toBe(true);
  });
  it('failure retains the same absolute request until explicit retry',async()=>{
    const e=setup();await e.session.load('task');e.fail();await expect(e.session.setCompleted('a',true)).rejects.toThrow('write');
    expect(e.session.hasPending()).toBe(true);await expect(e.session.setCompleted('b',true)).rejects.toThrow('RELOAD_REQUIRED');
    e.recover();await e.session.retry();expect(e.calls[1]).toEqual(e.calls[0]);expect(e.session.hasPending()).toBe(false);
  });
  it('lost response retries the same payload, not an inverted toggle',async()=>{
    const e=setup();await e.session.load('task');e.lose();await expect(e.session.setCompleted('a',true)).rejects.toThrow('reply lost');
    await e.session.retry();expect(e.calls[1]).toEqual(e.calls[0]);expect(e.state.items.items[0].completed).toBe(true);
  });
  it('reload abandons uncertain requests and bases future actions on current state',async()=>{
    const e=setup();await e.session.load('task');e.lose();await expect(e.session.setCompleted('a',true)).rejects.toThrow();
    await e.session.load('task');await expect(e.session.retry()).rejects.toThrow('RELOAD_REQUIRED');
    await e.session.setCompleted('a',false);expect(e.calls[1].operation_uuid).not.toBe(e.calls[0].operation_uuid);
    expect(e.calls[1].items[0].completed).toBe(false);
  });
  it('failed post-commit read cannot trigger a second write',async()=>{
    const e=setup();await e.session.load('task');await e.session.setCompleted('a',true);e.failRead();
    await expect(e.session.load('task')).rejects.toThrow('read');await expect(e.session.retry()).rejects.toThrow('RELOAD_REQUIRED');
    await expect(e.session.setCompleted('b',true)).rejects.toThrow('RELOAD_REQUIRED');expect(e.calls).toHaveLength(1);
  });
  it('read-only, absent, deleted and unchanged items never write',async()=>{
    const e=setup();await expect(e.session.setCompleted('a',true)).rejects.toThrow();
    e.state.read_only=true;await e.session.load('task');await expect(e.session.setCompleted('a',true)).rejects.toThrow();
    e.state.read_only=false;await e.session.load('task');
    await expect(e.session.setCompleted('missing',true)).rejects.toThrow('ITEM_MISSING');
    await expect(e.session.setCompleted('deleted',true)).rejects.toThrow('ITEM_MISSING');
    await e.session.setCompleted('a',false);expect(e.calls).toHaveLength(0);
  });
  it('overlapping writes and loads cannot discard an in-flight request',async()=>{
    const e=setup();let finish!:(n:number)=>void;e.api.save=()=>new Promise(r=>{finish=r;});await e.session.load('task');
    const pending=e.session.setCompleted('a',true);
    await expect(e.session.retry()).rejects.toThrow('BUSY');await expect(e.session.load('task')).rejects.toThrow('BUSY');
    finish(2);await pending;expect(e.session.canWrite()).toBe(false);
  });
  it('rejects mismatched and superseded read snapshots',async()=>{
    const e=setup();await expect(e.session.load('other')).rejects.toThrow('MISMATCH');
    const queue:((s:ChecklistPanelSnapshot)=>void)[]=[];e.api.read=()=>new Promise(r=>queue.push(r));
    const a=e.session.load('task');const assertion=expect(a).rejects.toThrow('SUPERSEDED');const b=e.session.load('task');
    queue[1](structuredClone(e.state));await b;queue[0](structuredClone(e.state));await assertion;expect(e.session.canWrite()).toBe(true);
  });
});
