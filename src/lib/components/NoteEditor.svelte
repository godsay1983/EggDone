<script lang="ts">
  import { onMount } from "svelte";
  import type { Note, NoteColor } from "$lib/types";

  export let note: Note;
  export let draft = false;
  export let saving = false;
  export let error: string | null = null;
  export let onChange: (note: Note, title: string, content: string) => void;
  export let onDone: () => Promise<void>;
  export let onPin: (note: Note, pinned: boolean) => Promise<void>;
  export let onColor: (note: Note, color: NoteColor) => Promise<void>;
  export let onDelete: (note: Note) => Promise<void>;

  const colors: NoteColor[] = ["default", "yellow", "pink", "green", "blue"];
  let title = note.title;
  let content = note.content;
  let titleInput: HTMLInputElement;

  onMount(() => {
    titleInput.focus();
    titleInput.select();
  });

  function changed() {
    onChange(note, title, content);
  }
</script>

<section class="note-editor" data-note-color={note.color} aria-label="编辑便签">
  <header>
    <button type="button" onclick={() => void onDone()}>返回</button>
    <span>{error ? error : saving ? "保存中…" : draft ? "开始输入后自动保存" : "已保存在本机"}</span>
    <button class="primary" type="button" onclick={() => void onDone()}>完成</button>
  </header>
  <input
    bind:this={titleInput}
    bind:value={title}
    maxlength="100"
    placeholder="便签标题"
    aria-label="便签标题"
    oninput={changed}
  />
  <textarea
    bind:value={content}
    maxlength="20000"
    placeholder="写下想法、资料或临时记录…"
    aria-label="便签内容"
    oninput={changed}
  ></textarea>
  <footer>
    <div class="note-color-picker" aria-label="便签颜色">
      {#each colors as color}
        <button
          class:active={note.color === color}
          data-note-color={color}
          type="button"
          aria-label={`切换为${color}颜色`}
          onclick={() => void onColor(note, color)}
        ></button>
      {/each}
    </div>
    <button type="button" onclick={() => void onPin(note, !note.pinned)}>{note.pinned ? "取消置顶" : "置顶"}</button>
    <button class="danger" type="button" onclick={() => void onDelete(note)}>{draft ? "放弃" : "删除"}</button>
  </footer>
</section>
