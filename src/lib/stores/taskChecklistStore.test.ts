import { describe, expect, it, vi } from 'vitest';
import { createChecklistSession } from './taskChecklistStore';
import type { ChecklistPanelSnapshot } from '$lib/types/taskChecklist';
const snapshot = (): ChecklistPanelSnapshot => ({todo_uuid:'todo',title:'original',note:'note',updated_at:1,read_only:false,items:{format_version:1,items:[]}});
function fixture() {
  const api={read:vi.fn().mockResolvedValue(snapshot()),save:vi.fn().mockResolvedValue(2),progress:vi.fn()};
  let id=0;
  const changed=vi.fn();
  return {api,changed,session:createChecklistSession(api,changed,()=>('id-'+ ++id) as ReturnType<typeof crypto.randomUUID>)};
}
describe('checklist draft session',()=>{
  it('keeps baseline isolated and does not write when loading or abandoning a draft',async()=>{
    const {api,session}=fixture();const s=await session.load('todo');s.title='draft';s.updated_at=100;
    expect(api.save).not.toHaveBeenCalled();await session.save('new','note',[]);
    expect(api.save.mock.calls[0][0]).toMatchObject({expected_updated_at:1,title:'new'});
  });
  it('retries the identical request after an uncertain failure, changes identity when edited',async()=>{
    const {api,session}=fixture();await session.load('todo');
    api.save.mockRejectedValueOnce(Error('offline'));
    await expect(session.save(' new ','note',[])).rejects.toThrow('offline');
    await session.save('new','note',[]);
    expect(api.save.mock.calls[0][0]).toEqual(api.save.mock.calls[1][0]);
    await session.save('changed','note',[]);
    expect(api.save.mock.calls[2][0].operation_uuid).not.toBe(api.save.mock.calls[1][0].operation_uuid);
  });
  it('blocks concurrent requests and freezes caller-owned items',async()=>{
    const {api,session}=fixture();await session.load('todo');let finish!:()=>void;
    api.save.mockImplementationOnce(()=>new Promise<void>(r=>finish=r));
    const items=[{uuid:'item',content:'keep',completed:false,sort_order:1000}];
    const pending=session.save('new','note',items);items[0].content='changed';
    await expect(session.save('new','note',items)).rejects.toThrow();
    expect(api.save).toHaveBeenCalledOnce();expect(api.save.mock.calls[0][0].items[0].content).toBe('keep');
    finish();await pending;
  });
  it('never writes after a failed load or for an archived task',async()=>{
    const {api,session}=fixture();api.read.mockRejectedValueOnce(Error('read'));
    await expect(session.load('todo')).rejects.toThrow();
    await expect(session.save('new','',[])).rejects.toThrow();
    api.read.mockResolvedValue({...snapshot(),read_only:true});await session.load('todo');
    await expect(session.save('new','',[])).rejects.toThrow();expect(api.save).not.toHaveBeenCalled();
  });
  it('does not report a committed save as failed when sync scheduling throws',async()=>{
    const {changed,session}=fixture();await session.load('todo');changed.mockImplementation(()=>{throw Error('sync');});
    await expect(session.save('new','',[])).resolves.toBeUndefined();
  });
});
