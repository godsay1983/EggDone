<script lang="ts">
  import { languageState, translator, type TranslationKey } from "$lib/i18n";
  import { formatDate } from "$lib/i18n/formatters";
  import type { Note, NoteAttachment, NoteColor } from "$lib/types";

  export let note: Note;
  export let onOpen: (note: Note) => void;
  export let onPin: (note: Note, pinned: boolean) => Promise<void>;
  export let onColor: (note: Note, color: NoteColor) => Promise<void>;
  export let onDelete: (note: Note) => Promise<void>;
  export let attachments: NoteAttachment[] = [];
  export let attachmentPreviewUrls: Record<string, string> = {};

  const colors: NoteColor[] = ["default", "yellow", "pink", "green", "blue"];
  let menuOpen = false;

  $: preview = note.content.trim() || $translator("note.openHint");
  $: title = note.title.trim() || preview.split(/\r?\n/, 1)[0] || $translator("note.untitled");
  $: imageAttachments = attachments.filter((attachment) => attachment.kind === "image");
  $: fileAttachments = attachments.filter((attachment) => attachment.kind === "file");

  function fileKind(attachment: NoteAttachment) {
    const extension = attachment.display_name.split(".").pop()?.toUpperCase();
    return extension && extension.length <= 8 ? extension : "FILE";
  }

  function colorName(color: NoteColor) {
    return $translator(`note.color${color === "default" ? "Default" : color[0].toUpperCase() + color.slice(1)}` as TranslationKey);
  }
</script>

<article class="note-card" data-note-color={note.color}>
  <button class="note-card-body" type="button" onclick={() => onOpen(note)}>
    <div class="note-card-heading">
      <strong>{title}</strong>
      {#if note.pinned}<span title={$translator("note.pinned")}>{$translator("note.pin")}</span>{/if}
    </div>
    <p class:with-attachments={attachments.length > 0}>{preview}</p>
    {#if attachments.length > 0}
      {#if imageAttachments.length > 0}
        <div class="note-card-media-summary">
          <div class="note-card-media-preview">
            {#if attachmentPreviewUrls[imageAttachments[0].uuid]}
              <img src={attachmentPreviewUrls[imageAttachments[0].uuid]} alt="" aria-hidden="true" />
            {:else}
              <span>{$translator("attachment.image")}</span>
            {/if}
          </div>
          <span class="note-card-media-info">
            <strong>{$translator("note.imagesCount", { count: imageAttachments.length })}</strong>
            <small title={imageAttachments[0].display_name}>{imageAttachments[0].display_name}</small>
            {#if fileAttachments.length > 0}<em>{$translator("note.otherFiles", { count: fileAttachments.length })}</em>{/if}
          </span>
        </div>
      {:else if fileAttachments.length > 0}
        <div class="note-card-file-summary">
          <span>{fileKind(fileAttachments[0])}</span>
          <strong title={fileAttachments[0].display_name}>{fileAttachments[0].display_name}</strong>
          {#if fileAttachments.length > 1}<em>+{fileAttachments.length - 1}</em>{/if}
        </div>
      {/if}
    {/if}
    <small>{formatDate(note.updated_at, { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }, $languageState.resolvedLocale)}</small>
  </button>
  <div class="note-card-actions">
    <button type="button" title={note.pinned ? $translator("note.unpin") : $translator("note.pin")} onclick={() => void onPin(note, !note.pinned)}>
      {note.pinned ? $translator("note.unpin") : $translator("note.pin")}
    </button>
    <button type="button" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>{$translator("note.changeColor")}</button>
    <button class="danger" type="button" onclick={() => void onDelete(note)}>{$translator("common.delete")}</button>
  </div>
  {#if menuOpen}
    <div class="note-color-picker" aria-label={$translator("note.color")}>
      {#each colors as color}
        <button
          class:active={note.color === color}
          data-note-color={color}
          type="button"
          aria-label={$translator("note.changeToColor", { color: colorName(color) })}
          onclick={() => { menuOpen = false; void onColor(note, color); }}
        ></button>
      {/each}
    </div>
  {/if}
</article>
