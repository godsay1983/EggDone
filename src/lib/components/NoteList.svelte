<script lang="ts">
  import { translator } from "$lib/i18n";
  import type { Note, NoteAttachment, NoteColor } from "$lib/types";
  import NoteCard from "./NoteCard.svelte";

  export let items: Note[];
  export let loading = false;
  export let error: string | null = null;
  export let searchActive = false;
  export let onOpen: (note: Note) => void;
  export let onPin: (note: Note, pinned: boolean) => Promise<void>;
  export let onColor: (note: Note, color: NoteColor) => Promise<void>;
  export let onDelete: (note: Note) => Promise<void>;
  export let attachmentsByNote: Record<string, NoteAttachment[]> = {};
  export let attachmentPreviewUrls: Record<string, string> = {};
</script>

<section class="note-list" aria-live="polite">
  {#if loading}
    <div class="status">{$translator("note.loading")}</div>
  {:else if error && items.length === 0}
    <div class="status error">{error}</div>
  {:else if items.length === 0}
    <div class="empty-state note-empty">
      <img class="empty-mascot" src="/eggdone-icon.png" alt="" aria-hidden="true" />
      <strong>{searchActive ? $translator("note.noMatch") : $translator("note.emptyTitle")}</strong>
      <span>{searchActive ? $translator("note.searchHint") : $translator("note.emptyHint")}</span>
    </div>
  {:else}
    {#if error}<div class="inline-error" role="alert">{error}</div>{/if}
    {#each items as note (note.uuid)}
      <NoteCard
        {note}
        {onOpen}
        {onPin}
        {onColor}
        {onDelete}
        attachments={attachmentsByNote[note.uuid] ?? []}
        {attachmentPreviewUrls}
      />
    {/each}
  {/if}
</section>
