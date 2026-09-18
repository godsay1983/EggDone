import { invoke } from "@tauri-apps/api/core";
import type { TrashKind } from "./trashApi";

export interface PurgeTarget { kind: TrashKind; uuid: string }
export interface PurgePlan {
  operation_uuid: string;
  total: number;
  attachments: number;
  bytes: number;
  state: "prepared" | "running" | "complete";
  pending: number;
  purged: number;
  skipped: number;
  cleanup_pending: number;
  sync_pending: boolean;
  remote_pending: number;
}
export const purgeApi = {
  prepare: (selected: PurgeTarget[] | null) => invoke<PurgePlan>("prepare_trash_purge", { selected }),
  run: (operationUuid: string) => invoke<PurgePlan>("run_trash_purge", { operationUuid }),
  unfinished: () => invoke<PurgePlan | null>("unfinished_trash_purge"),
  status: (operationUuid: string) => invoke<PurgePlan>("trash_purge_status", { operationUuid }),
};
