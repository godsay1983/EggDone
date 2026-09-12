export type AttachmentStatus =
  | "upload_failed" | "download_failed" | "pending_upload" | "uploading"
  | "metadata_pending" | "downloading" | "preview_ready" | "download_needed"
  | "available" | "unknown";
export type AttachmentRetry = "upload" | "preview" | "original";
export interface AttachmentPresentationInput {
  state: string;
  remoteUploaded: boolean;
  hasOriginal: boolean;
  hasPreview: boolean;
  hasRemotePreview: boolean;
  isImage: boolean;
}
export function attachmentStatus(input: AttachmentPresentationInput): AttachmentStatus {
  if (input.state === "failed") return input.remoteUploaded ? "download_failed" : "upload_failed";
  if (input.state === "uploading") return "uploading";
  if (input.state === "pending_upload") return "pending_upload";
  if (input.state === "uploaded") return "metadata_pending";
  if (input.state === "downloading") return "downloading";
  if (input.state !== "synced" && input.state !== "cached" && input.state !== "remote_only") return "unknown";
  if (!input.remoteUploaded) return "unknown";
  if (input.hasOriginal) return "available";
  if (input.isImage && input.hasPreview) return "preview_ready";
  return "download_needed";
}
// A preview retry must have remote preview metadata, otherwise fetch the original.
export function attachmentRetry(input: AttachmentPresentationInput): AttachmentRetry {
  if (!input.remoteUploaded) return "upload";
  return input.isImage && !input.hasPreview && input.hasRemotePreview ? "preview" : "original";
}
export type AttachmentFailureHint = "permission" | "network" | "integrity" | "missing" | "unknown";
export function attachmentFailureHint(error: string | null): AttachmentFailureHint {
  const text = (error ?? "").toLowerCase();
  // Return a fixed localized category, never display raw URLs, paths or credentials here.
  if (/403|401|accessdenied|permission|credential|权限|凭据/.test(text)) return "permission";
  if (/sha256|checksum|integrity|校验/.test(text)) return "integrity";
  if (/404|nosuchkey|enoent|not found|不存在/.test(text)) return "missing";
  if (/timeout|network|connection|dns|offline|超时|网络|连接/.test(text)) return "network";
  return "unknown";
}
