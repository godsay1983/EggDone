import { invoke } from "@tauri-apps/api/core";

import type { NoteAttachment } from "$lib/types";

export interface NoteAttachmentCacheStats {
  totalBytes: number;
  reclaimableBytes: number;
  protectedBytes: number;
  fileCount: number;
  reclaimableFileCount: number;
  protectedFileCount: number;
  pendingCount: number;
}

function bytesToUrl(bytes: number[], mimeType: string) {
  return URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: mimeType }));
}

export const noteAttachmentApi = {
  list(noteUuid: string): Promise<NoteAttachment[]> {
    return invoke<NoteAttachment[]>("list_note_attachments", { noteUuid });
  },

  reorder(noteUuid: string, orderedUuids: string[]): Promise<NoteAttachment[]> {
    return invoke<NoteAttachment[]>("reorder_note_attachments", { noteUuid, orderedUuids });
  },

  async createImage(noteUuid: string, file: File): Promise<NoteAttachment> {
    const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
    return invoke<NoteAttachment>("create_note_image_attachment", {
      noteUuid,
      displayName: file.name,
      bytes,
    });
  },

  async previewUrl(attachment: NoteAttachment): Promise<string> {
    const bytes = await invoke<number[]>("read_note_attachment_preview", {
      uuid: attachment.uuid,
    });
    return bytesToUrl(bytes, attachment.preview_mime_type ?? "image/jpeg");
  },

  async originalUrl(attachment: NoteAttachment): Promise<string> {
    const bytes = await invoke<number[]>("read_note_attachment_original", {
      uuid: attachment.uuid,
    });
    return bytesToUrl(bytes, attachment.mime_type);
  },

  delete(uuid: string): Promise<NoteAttachment> {
    return invoke<NoteAttachment>("delete_note_attachment", { uuid });
  },

  restore(uuid: string): Promise<NoteAttachment> {
    return invoke<NoteAttachment>("restore_note_attachment", { uuid });
  },

  retry(uuid: string): Promise<NoteAttachment> {
    return invoke<NoteAttachment>("retry_note_attachment", { uuid });
  },

  cacheStats(): Promise<NoteAttachmentCacheStats> {
    return invoke<NoteAttachmentCacheStats>("get_note_attachment_cache_stats");
  },

  clearCache(): Promise<NoteAttachmentCacheStats> {
    return invoke<NoteAttachmentCacheStats>("clear_note_attachment_cache");
  },
};
