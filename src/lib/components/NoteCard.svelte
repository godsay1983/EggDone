<script lang="ts">
  import type { Note, NoteColor } from "$lib/types";

  export let note: Note;
  export let onOpen: (note: Note) => void;
  export let onPin: (note: Note, pinned: boolean) => Promise<void>;
  export let onColor: (note: Note, color: NoteColor) => Promise<void>;
  export let onDelete: (note: Note) => Promise<void>;

  const colors: NoteColor[] = ["default", "yellow", "pink", "green", "blue"];
  let menuOpen = false;

  $: preview = note.content.trim() || "点击打开便签";
  $: title = note.title.trim() || preview.split(/\r?\n/, 1)[0] || "无标题便签";
</script>

<article class="note-card" data-note-color={note.color}>
  <button class="note-card-body" type="button" onclick={() => onOpen(note)}>
    <div class="note-card-heading">
      <strong>{title}</strong>
      {#if note.pinned}<span title="已置顶">置顶</span>{/if}
    </div>
    <p>{preview}</p>
    <small>{new Date(note.updated_at).toLocaleString("zh-CN", { month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" })}</small>
  </button>
  <div class="note-card-actions">
    <button type="button" title={note.pinned ? "取消置顶" : "置顶"} onclick={() => void onPin(note, !note.pinned)}>
      {note.pinned ? "取消置顶" : "置顶"}
    </button>
    <button type="button" aria-expanded={menuOpen} onclick={() => (menuOpen = !menuOpen)}>换色</button>
    <button class="danger" type="button" onclick={() => void onDelete(note)}>删除</button>
  </div>
  {#if menuOpen}
    <div class="note-color-picker" aria-label="便签颜色">
      {#each colors as color}
        <button
          class:active={note.color === color}
          data-note-color={color}
          type="button"
          aria-label={`切换为${color}颜色`}
          onclick={() => { menuOpen = false; void onColor(note, color); }}
        ></button>
      {/each}
    </div>
  {/if}
</article>
