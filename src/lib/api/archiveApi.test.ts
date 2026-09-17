import { beforeEach,expect,it,vi } from "vitest";
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock("@tauri-apps/api/core",()=>({invoke}));
import {archiveApi,archiveBatchApi} from "./archiveApi";
beforeEach(()=>{ invoke.mockReset(); });
it("keeps batch commands identity-only and never accepts caller clocks",async()=>{
  const request={operation:"batch",action:"delete" as const,targets:[{uuid:"task",scope:"scope",fingerprint:"hash"}]};
  await archiveBatchApi.prepare(request);await archiveBatchApi.run("batch");await archiveBatchApi.pending();await archiveBatchApi.dismiss("batch");
  expect(invoke.mock.calls).toEqual([["prepare_archive_batch",request],["run_archive_batch",{operation:"batch"}],
    ["pending_archive_batches"],["dismiss_archive_batch",{operation:"batch"}]]);
});
it("maps archive commands without accepting a timestamp or device identity",async()=>{
  await archiveApi.list("test",null);await archiveApi.preview("task");
  const request={operation:"operation",action:"reopen" as const,expected:{uuid:"task",scope:"scope",fingerprint:"hash"}};
  await archiveApi.apply(request);
  expect(invoke.mock.calls).toEqual([["list_archived",{query:"test",cursor:null}],["preview_archived",{uuid:"task"}],
    ["apply_archive_action",request]]);
});
it("does not mask failed reads or writes",async()=>{
  invoke.mockRejectedValue("ARCHIVE_CONFLICT");
  await expect(archiveApi.list("",null)).rejects.toBe("ARCHIVE_CONFLICT");
  await expect(archiveApi.preview("task")).rejects.toBe("ARCHIVE_CONFLICT");
});
