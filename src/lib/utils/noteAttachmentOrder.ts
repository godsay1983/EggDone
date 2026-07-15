import type { NoteAttachment } from "$lib/types";

export function reorderNoteAttachmentWithinKind(
  attachments: NoteAttachment[],
  attachmentUuid: string,
  direction: -1 | 1,
): NoteAttachment[] | null {
  const currentIndex = attachments.findIndex((item) => item.uuid === attachmentUuid);
  if (currentIndex < 0) return null;

  const current = attachments[currentIndex];
  const sameKind = attachments.filter((item) => item.kind === current.kind);
  const kindIndex = sameKind.findIndex((item) => item.uuid === attachmentUuid);
  const target = sameKind[kindIndex + direction];
  if (!target) return null;

  const targetIndex = attachments.findIndex((item) => item.uuid === target.uuid);
  const reordered = [...attachments];
  [reordered[currentIndex], reordered[targetIndex]] = [reordered[targetIndex], reordered[currentIndex]];
  return reordered;
}
