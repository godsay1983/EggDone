import { expect,it,vi } from "vitest";
import { createArchiveBatchStore } from "./archiveBatchStore";
import type { ArchiveBatchRequest,ArchiveJob,ArchivePreview } from "$lib/api/archiveApi";
const items=Array.from({length:3},(_,i)=>({expected:{uuid:String(i),scope:"scope",fingerprint:"hash"}} as ArchivePreview));
function fixture(){
  let saved:ArchiveJob|null=null;
  const port={
    prepare:vi.fn(async(r:ArchiveBatchRequest)=>{saved??={operation_uuid:r.operation,action:r.action,scope:"scope",targets:structuredClone(r.targets),results:[]};return structuredClone(saved);}),
    run:vi.fn(async()=>{const uuid=saved!.targets[saved!.results.length].uuid;saved!.results.push({uuid,error:uuid==="1"?"ARCHIVE_CONFLICT":null,
      result:uuid==="1"?null:{uuid,action:saved!.action,outcome:"applied",result_version:2,warnings:[]}});return structuredClone(saved!);}),
    pending:vi.fn(async()=>saved?[structuredClone(saved)]:[]),dismiss:vi.fn(async()=>{})
  };
  return {port,store:createArchiveBatchStore(port,()=>"batch")};
}
it("prepares without writes, snapshots targets, and completes in bounded chunks",async()=>{
 const {port,store}=fixture(),copy=structuredClone(items),r=store.prepare("unarchive",copy);copy[0].expected.fingerprint="new";
 expect(port.prepare).not.toHaveBeenCalled();expect(r.targets[0].fingerprint).toBe("hash");
 const progress=vi.fn(),refresh=vi.fn(async()=>{}),out=await store.execute(r,refresh,progress);
 expect(out.error).toBeNull();expect(out.job?.results).toHaveLength(3);expect(out.job?.results[1].error).toBe("ARCHIVE_CONFLICT");
 expect(port.run).toHaveBeenCalledTimes(3);expect(progress).toHaveBeenCalledTimes(4);expect(refresh).toHaveBeenCalledOnce();
});
it("retries the same persisted batch after a committed chunk loses its reply",async()=>{
 const {port,store}=fixture(),r=store.prepare("delete",items),refresh=vi.fn(async()=>{}),run=port.run.getMockImplementation()!;
 port.run.mockImplementationOnce(async()=>{await run();throw Error("reply lost");});
 expect((await store.execute(r,refresh,()=>{})).error).toBe("reply lost");
 const resumed=createArchiveBatchStore(port),job=(await resumed.pending())[0];expect(job.results).toHaveLength(1);
 const out=await resumed.execute({operation:job.operation_uuid,action:job.action,targets:job.targets},refresh,()=>{});
 expect(out.job?.results).toHaveLength(3);expect(port.run).toHaveBeenCalledTimes(3);
 expect(port.prepare.mock.calls[0]).toEqual(port.prepare.mock.calls[1]);expect(refresh).toHaveBeenCalledTimes(2);
});
it("retains completed results on refresh failure without running chunks again",async()=>{
 const {port,store}=fixture(),r=store.prepare("delete",items);
 expect((await store.execute(r,async()=>{throw Error("refresh");},()=>{})).refreshNeeded).toBe(true);
 expect((await store.pending())[0].results).toHaveLength(3);
 await store.execute(r,async()=>{},()=>{});expect(port.run).toHaveBeenCalledTimes(3);
 await store.dismiss(r.operation);expect(port.dismiss).toHaveBeenCalledWith("batch");
});
it("rejects concurrent execution and copies the request before awaiting",async()=>{
 const {port,store}=fixture(),r=store.prepare("delete",items);let release!:()=>void;
 const prepare=port.prepare.getMockImplementation()!;
 port.prepare.mockImplementationOnce(async v=>{await new Promise<void>(done=>release=done);return prepare(v);});
 const first=store.execute(r,async()=>{},()=>{});r.targets[0].fingerprint="changed";
 await expect(store.execute(r,async()=>{},()=>{})).rejects.toThrow("ARCHIVE_BUSY");release();await first;
 expect(port.prepare.mock.calls[0][0].targets[0].fingerprint).toBe("hash");
});
it("refreshes after prepare failure and surfaces errors without creating new identities",async()=>{
 const {port,store}=fixture(),r=store.prepare("delete",items),refresh=vi.fn(async()=>{});
 port.prepare.mockRejectedValueOnce(Error("ARCHIVE_SCOPE_CHANGED"));
 const out=await store.execute(r,refresh,()=>{});expect(out.error).toBe("ARCHIVE_SCOPE_CHANGED");expect(port.run).not.toHaveBeenCalled();expect(refresh).toHaveBeenCalledOnce();
});
