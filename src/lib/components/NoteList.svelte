<script lang="ts">
  import type { Note, NoteColor } from "$lib/types";
  import NoteCard from "./NoteCard.svelte";

  export let items: Note[];
  export let loading = false;
  export let error: string | null = null;
  export let searchActive = false;
  export let onOpen: (note: Note) => void;
  export let onPin: (note: Note, pinned: boolean) => Promise<void>;
  export let onColor: (note: Note, color: NoteColor) => Promise<void>;
  export let onDelete: (note: Note) => Promise<void>;
</script>

<section class="note-list" aria-live="polite">
  {#if loading}
    <div class="status">正在整理便签…</div>
  {:else if error && items.length === 0}
    <div class="status error">{error}</div>
  {:else if items.length === 0}
    <div class="empty-state note-empty">
      <img class="empty-mascot" src="/eggdone-icon.png" alt="" aria-hidden="true" />
      <strong>{searchActive ? "没有找到匹配便签" : "随手记下一点想法"}</strong>
      <span>{searchActive ? "换个关键词试试" : "便签会自动保存在本机"}</span>
    </div>
  {:else}
    {#if error}<div class="inline-error" role="alert">{error}</div>{/if}
    {#each items as note (note.uuid)}
      <NoteCard {note} {onOpen} {onPin} {onColor} {onDelete} />
    {/each}
  {/if}
</section>
