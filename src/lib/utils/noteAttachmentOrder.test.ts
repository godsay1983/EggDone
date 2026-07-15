import { describe, expect, it } from "vitest";

import type { NoteAttachment } from "$lib/types";
import { reorderNoteAttachmentWithinKind } from "./noteAttachmentOrder";

function attachment(uuid: string, kind: "image" | "file"): NoteAttachment {
  return {
    id: 0,
    uuid,
    note_uuid: "note-1",
    kind,
    display_name: `${uuid}.${kind === "image" ? "jpg" : "md"}`,
    mime_type: kind === "image" ? "image/jpeg" : "text/markdown",
    byte_size: 1,
    sha256: uuid,
    preview_mime_type: null,
    preview_byte_size: null,
    preview_sha256: null,
    width: null,
    height: null,
    sort_order: 0,
    created_at: 0,
    updated_at: 0,
    deleted_at: null,
    updated_by: "test",
    local_original_path: null,
    local_preview_path: null,
    transfer_state: "synced",
    transfer_error: null,
    remote_uploaded: true,
  };
}

describe("reorderNoteAttachmentWithinKind", () => {
  const interleaved = [
    attachment("image-1", "image"),
    attachment("file-1", "file"),
    attachment("image-2", "image"),
    attachment("file-2", "file"),
  ];

  it("moves images without changing file order", () => {
    const result = reorderNoteAttachmentWithinKind(interleaved, "image-2", -1);
    expect(result?.map((item) => item.uuid)).toEqual(["image-2", "file-1", "image-1", "file-2"]);
  });

  it("moves files without changing image order", () => {
    const result = reorderNoteAttachmentWithinKind(interleaved, "file-1", 1);
    expect(result?.map((item) => item.uuid)).toEqual(["image-1", "file-2", "image-2", "file-1"]);
  });

  it("does not move beyond the current kind boundary", () => {
    expect(reorderNoteAttachmentWithinKind(interleaved, "image-1", -1)).toBeNull();
    expect(reorderNoteAttachmentWithinKind(interleaved, "file-2", 1)).toBeNull();
  });
});
