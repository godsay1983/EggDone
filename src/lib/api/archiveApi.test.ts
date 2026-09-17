import { beforeEach,expect,it,vi } from "vitest";
const {invoke}=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock("@tauri-apps/api/core",()=>({invoke}));
import {archiveApi} from "./archiveApi";
beforeEach(()=>{ invoke.mockReset(); });
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
