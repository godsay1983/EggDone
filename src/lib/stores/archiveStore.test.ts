import { describe, expect, it, vi } from "vitest";
import { createArchiveStore, type ArchivePort } from "./archiveStore";
import type { ArchivePreview, ArchiveRequest } from "$lib/api/archiveApi";
const item: ArchivePreview = { expected: { uuid:"task",scope:"scope",fingerprint:"hash" }, title:"Task",content:"Body",
  completed:true,archived_at:1,updated_at:1,completed_at:1,due_date:null,due_at:null,group_name:null,
  checklist_json:'{"items":[]}',links_json:'{"links":[]}' };
function fixture() {
  const port: ArchivePort = {
    list:vi.fn(async () => ({items:[item],next:null})),preview:vi.fn(async () => structuredClone(item)),
    apply:vi.fn(async (r: ArchiveRequest) => ({ uuid:r.expected.uuid,action:r.action,outcome:"applied",result_version:2,warnings:[] }))
  };
  return {port,store:createArchiveStore(port,() => "operation")};
}
describe("archive session",() => {
  it("reads and prepares an immutable confirmation without writing",async () => {
    const {port,store}=fixture(); await store.list("Body",null); await store.preview(item);
    expect(port.list).toHaveBeenCalledWith("Body",null); expect(port.preview).toHaveBeenCalledWith("task");
    const copy=structuredClone(item),request=store.prepare("unarchive",copy); copy.expected.fingerprint="changed";
    expect(request.expected.fingerprint).toBe("hash"); expect(port.apply).not.toHaveBeenCalled();
  });
  it("retries an uncertain result with the same operation and no caller clock",async () => {
    const {port,store}=fixture(), request=store.prepare("reopen",item),refresh=vi.fn(async()=>{});
    vi.mocked(port.apply).mockRejectedValueOnce(Error("response lost"));
    await expect(store.apply(request,refresh)).rejects.toThrow("response lost"); expect(refresh).not.toHaveBeenCalled();
    await store.apply(request,refresh);
    expect(vi.mocked(port.apply).mock.calls[0]).toEqual(vi.mocked(port.apply).mock.calls[1]);
    expect(Object.keys(vi.mocked(port.apply).mock.calls[1][0]).sort()).toEqual(["action","expected","operation"]);
  });
  it("never turns failed refresh into a retryable write failure",async()=>{
    const {port,store}=fixture(), request=store.prepare("delete",item);
    expect((await store.apply(request,async()=>{throw Error("refresh");})).refreshNeeded).toBe(true);
    await store.list("",null); expect(port.apply).toHaveBeenCalledTimes(1);
  });
  it("blocks duplicate writes and snapshots before await",async()=>{
    const {port,store}=fixture(),request=store.prepare("unarchive",item);
    let release!:()=>void;
    vi.mocked(port.apply).mockImplementation(async r=>{
      await new Promise<void>(resolve=>release=resolve);
      return {uuid:r.expected.uuid,action:r.action,outcome:"applied",result_version:2,warnings:[]};
    });
    const first=store.apply(request,async()=>{});request.expected.fingerprint="changed";
    await expect(store.apply(request,async()=>{})).rejects.toThrow("ARCHIVE_BUSY");
    expect(vi.mocked(port.apply).mock.calls[0][0].expected.fingerprint).toBe("hash");
    release();await first;
  });
});
