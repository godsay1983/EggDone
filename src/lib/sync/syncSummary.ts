export type SyncSummaryState =
  | "local" | "unconfigured" | "idle" | "syncing" | "offline" | "conflict"
  | "failed" | "upload_failed" | "upload_pending" | "pending" | "synced";

export interface SyncSummaryInput {
  enabled: boolean;
  configured: boolean;
  busy: boolean;
  kind: string;
  dirty: boolean;
  pendingUploads: number;
  failedUploads: number;
}

export type SyncSummaryTone = "neutral" | "active" | "warning" | "success";

export function syncSummaryState(input: SyncSummaryInput): SyncSummaryState {
  // Availability is the applied configuration, never an unsaved settings draft.
  if (!input.enabled) return "local";
  if (!input.configured) return "unconfigured";
  if (input.busy || input.kind === "syncing") return "syncing";
  if (input.kind === "offline") return "offline";
  if (input.kind === "conflict") return "conflict";
  if (input.failedUploads > 0) return "upload_failed";
  if (input.kind === "failed") return "failed";
  if (input.dirty || input.kind === "pending") return "pending";
  if (input.pendingUploads > 0) return "upload_pending";
  if (input.kind === "synced") return "synced";
  return "idle";
}

export function syncSummaryTone(state: SyncSummaryState): SyncSummaryTone {
  if (state === "syncing") return "active";
  if (state === "synced") return "success";
  if (state === "local" || state === "idle") return "neutral";
  return "warning";
}
