<script lang="ts">
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

  $: preview = note.content.trim() || "点击打开便签";
  $: title = note.title.trim() || preview.split(/\r?\n/, 1)[0] || "无标题便签";
  $: imageAttachments = attachments.filter((attachment) => attachment.kind === "image");
  $: fileAttachments = attachments.filter((attachment) => attachment.kind === "file");

  function fileKind(attachment: NoteAttachment) {
    const extension = attachment.display_name.split(".").pop()?.toUpperCase();
    return extension && extension.length <= 8 ? extension : "FILE";
  }
</script>

<article class="note-card" data-note-color={note.color}>
  <button class="note-card-body" type="button" onclick={() => onOpen(note)}>
    <div class="note-card-heading">
      <strong>{title}</strong>
      {#if note.pinned}<span title="已置顶">置顶</span>{/if}
    </div>
    <p class:with-attachments={attachments.length > 0}>{preview}</p>
    {#if attachments.length > 0}
      {#if imageAttachments.length > 0}
        <div class="note-card-media-summary">
          <div class="note-card-media-preview">
            {#if attachmentPreviewUrls[imageAttachments[0].uuid]}
              <img src={attachmentPreviewUrls[imageAttachments[0].uuid]} alt="" aria-hidden="true" />
            {:else}
              <span>图片</span>
            {/if}
          </div>
          <span class="note-card-media-info">
            <strong>{imageAttachments.length} 张图片</strong>
            <small title={imageAttachments[0].display_name}>{imageAttachments[0].display_name}</small>
            {#if fileAttachments.length > 0}<em>另有 {fileAttachments.length} 个文件</em>{/if}
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
