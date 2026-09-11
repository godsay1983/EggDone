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
  let actionsOpen = false;
  let card: HTMLElement;
  let moreButton: HTMLButtonElement;

  function closeActions() {
    actionsOpen = false;
    menuOpen = false;
  }

  function handleEscape(event: KeyboardEvent) {
    if (event.key === "Escape" && actionsOpen && card?.contains(document.activeElement)) {
      event.preventDefault();
      closeActions();
      moreButton.focus();
    }
  }

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

<svelte:window onkeydown={handleEscape} onpointerdown={(event) => {
  if (event.target instanceof Node && !card?.contains(event.target)) closeActions();
}} />

<article bind:this={card} class="note-card" data-note-color={note.color}>
  <button class="note-card-body" type="button" onclick={() => { closeActions(); onOpen(note); }}
    oncontextmenu={(event) => { event.preventDefault(); actionsOpen = true; moreButton.focus(); }}>
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
  <div class="note-card-more">
    <button bind:this={moreButton} class="action-button" type="button" aria-expanded={actionsOpen}
      aria-controls={`note-actions-${note.uuid}`}
      onclick={() => { if (actionsOpen) closeActions(); else actionsOpen = true; }}>
      {$translator(actionsOpen ? "common.collapse" : "common.more")}
    </button>
  </div>
  {#if actionsOpen}
    <div class="note-card-actions" id={`note-actions-${note.uuid}`}>
      <button class="action-button" type="button" title={note.pinned ? $translator("note.unpin") : $translator("note.pin")} onclick={() => void onPin(note, !note.pinned)}>
        {note.pinned ? $translator("note.unpin") : $translator("note.pin")}
      </button>
      <button class="action-button" type="button" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>{$translator("note.changeColor")}</button>
      <button class="action-button" data-tone="danger" type="button" onclick={() => void onDelete(note)}>{$translator("common.delete")}</button>
    </div>
    {#if menuOpen}
      <div class="note-color-picker" aria-label={$translator("note.color")}>
        {#each colors as color}
          <button
            class:active={note.color === color}
            data-note-color={color}
            type="button"
            aria-label={$translator("note.changeToColor", { color: colorName(color) })}
            onclick={() => { menuOpen = false; moreButton.focus(); void onColor(note, color); }}
          ></button>
        {/each}
      </div>
    {/if}
  {/if}
</article>
