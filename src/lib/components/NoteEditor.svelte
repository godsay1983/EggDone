<script lang="ts">
  import { onMount } from "svelte";
  import type { Note, NoteAttachment, NoteColor } from "$lib/types";

  export let note: Note;
  export let draft = false;
  export let saving = false;
  export let error: string | null = null;
  export let onChange: (note: Note, title: string, content: string) => void;
  export let onDone: () => Promise<void>;
  export let onPin: (note: Note, pinned: boolean) => Promise<void>;
  export let onColor: (note: Note, color: NoteColor) => Promise<void>;
  export let onDelete: (note: Note) => Promise<void>;
  export let attachments: NoteAttachment[] = [];
  export let attachmentPreviewUrls: Record<string, string> = {};
  export let attachmentBusy = false;
  export let onAddImages: (files: File[]) => Promise<void>;
  export let onOpenAttachment: (attachment: NoteAttachment) => Promise<string>;
  export let onMoveAttachment: (attachment: NoteAttachment, direction: -1 | 1) => Promise<void>;
  export let onDeleteAttachment: (attachment: NoteAttachment) => Promise<void>;
  export let onRetryAttachment: (attachment: NoteAttachment) => Promise<void>;

  const colors: NoteColor[] = ["default", "yellow", "pink", "green", "blue"];
  let title = note.title;
  let content = note.content;
  let titleInput: HTMLInputElement;
  let fileInput: HTMLInputElement;
  let viewerAttachment: NoteAttachment | null = null;
  let viewerUrl = "";
  let viewerLoading = false;
  let viewerError = "";

  onMount(() => {
    titleInput.focus();
    titleInput.select();
  });

  function changed() {
    onChange(note, title, content);
  }

  function selectFiles(event: Event) {
    const files = Array.from((event.currentTarget as HTMLInputElement).files ?? []);
    if (files.length > 0) void onAddImages(files);
    (event.currentTarget as HTMLInputElement).value = "";
  }

  function pasteImages(event: ClipboardEvent) {
    const files = Array.from(event.clipboardData?.files ?? []).filter((file) => file.type.startsWith("image/"));
    if (files.length === 0) return;
    event.preventDefault();
    void onAddImages(files);
  }

  function dropImages(event: DragEvent) {
    event.preventDefault();
    const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file.type.startsWith("image/"));
    if (files.length > 0) void onAddImages(files);
  }

  async function openAttachment(attachment: NoteAttachment) {
    viewerLoading = true;
    viewerError = "";
    viewerAttachment = attachment;
    try {
      viewerUrl = await onOpenAttachment(attachment);
    } catch (reason) {
      viewerError = reason instanceof Error ? reason.message : String(reason);
    } finally {
      viewerLoading = false;
    }
  }

  function closeViewer() {
    if (viewerUrl) URL.revokeObjectURL(viewerUrl);
    viewerUrl = "";
    viewerError = "";
    viewerAttachment = null;
  }

  function attachmentState(attachment: NoteAttachment) {
    if (attachment.transfer_state === "failed") return "同步失败";
    if (attachment.transfer_state === "uploading") return "上传中";
    if (attachment.transfer_state === "pending_upload") return "等待同步";
    if (attachment.transfer_state === "remote_only") return "需要下载";
    return "已同步";
  }
</script>

<section
  class="note-editor"
  data-note-color={note.color}
  aria-label="编辑便签"
  ondragover={(event) => event.preventDefault()}
  ondrop={dropImages}
>
  <header>
    <button type="button" onclick={() => void onDone()}>返回</button>
    <span>{error ? error : saving ? "保存中…" : draft ? "开始输入后自动保存" : "已保存在本机"}</span>
    <input bind:this={fileInput} class="note-file-input" type="file" accept="image/jpeg,image/png,image/webp" multiple onchange={selectFiles} />
    <button class="attachment-trigger" type="button" title="添加图片" aria-label="添加图片" disabled={attachmentBusy} onclick={() => fileInput.click()}>▧</button>
    <button class="primary" type="button" onclick={() => void onDone()}>完成</button>
  </header>
  <input
    bind:this={titleInput}
    bind:value={title}
    maxlength="100"
    placeholder="便签标题"
    aria-label="便签标题"
    autocomplete="off"
    oninput={changed}
  />
  <textarea
    bind:value={content}
    maxlength="20000"
    placeholder="写下想法、资料或临时记录…"
    aria-label="便签内容"
    autocomplete="off"
    oninput={changed}
    onpaste={pasteImages}
  ></textarea>
  {#if attachments.length > 0}
    <section class="note-attachment-grid" aria-label="便签图片">
      {#each attachments as attachment, index (attachment.uuid)}
        <article class:failed={attachment.transfer_state === "failed"}>
          <button class="note-attachment-preview" type="button" onclick={() => void openAttachment(attachment)}>
            {#if attachmentPreviewUrls[attachment.uuid]}
              <img src={attachmentPreviewUrls[attachment.uuid]} alt={attachment.display_name} />
            {:else}
              <span>{attachment.transfer_state === "remote_only" ? "下载预览" : "正在准备"}</span>
            {/if}
          </button>
          <div>
            <small title={attachment.transfer_error ?? attachment.display_name}>{attachmentState(attachment)}</small>
            {#if attachment.transfer_state === "failed"}
              <button type="button" disabled={attachmentBusy} onclick={() => void onRetryAttachment(attachment)}>重试</button>
            {:else}
              <button type="button" disabled={index === 0 || attachmentBusy} onclick={() => void onMoveAttachment(attachment, -1)}>前移</button>
              <button type="button" disabled={index === attachments.length - 1 || attachmentBusy} onclick={() => void onMoveAttachment(attachment, 1)}>后移</button>
            {/if}
            <button class="danger" type="button" disabled={attachmentBusy} onclick={() => void onDeleteAttachment(attachment)}>删除</button>
          </div>
        </article>
      {/each}
    </section>
  {/if}
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

{#if viewerAttachment}
  <div class="note-image-viewer" role="dialog" aria-modal="true" aria-label="查看图片" tabindex="-1">
    <div>
      <header>
        <strong>{viewerAttachment.display_name}</strong>
        <button type="button" onclick={closeViewer}>关闭</button>
      </header>
      {#if viewerLoading}
        <span>正在读取原图…</span>
      {:else if viewerUrl}
        <img src={viewerUrl} alt={viewerAttachment.display_name} />
        <a href={viewerUrl} download={viewerAttachment.display_name}>保存原图</a>
      {:else}
        <span>{viewerError || "原图暂不可用"}</span>
      {/if}
    </div>
  </div>
{/if}
