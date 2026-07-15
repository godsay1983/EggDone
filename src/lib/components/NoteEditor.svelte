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
  export let onAddFiles: (files: File[]) => Promise<void>;
  export let onOpenAttachment: (attachment: NoteAttachment) => Promise<string>;
  export let onOpenFile: (attachment: NoteAttachment) => Promise<void>;
  export let onMoveAttachment: (attachment: NoteAttachment, direction: -1 | 1) => Promise<void>;
  export let onDeleteAttachment: (attachment: NoteAttachment) => Promise<void>;
  export let onRetryAttachment: (attachment: NoteAttachment) => Promise<void>;

  const colors: NoteColor[] = ["default", "yellow", "pink", "green", "blue"];
  let title = note.title;
  let content = note.content;
  let titleInput: HTMLInputElement;
  let imageInput: HTMLInputElement;
  let attachmentInput: HTMLInputElement;
  let viewerAttachment: NoteAttachment | null = null;
  let viewerUrl = "";
  let viewerLoading = false;
  let viewerError = "";
  let fileActionError = "";
  let attachmentManagerOpen = false;
  let addMenuOpen = false;

  $: imageAttachments = attachments.filter((attachment) => attachment.kind === "image");
  $: fileAttachments = attachments.filter((attachment) => attachment.kind === "file");

  onMount(() => {
    titleInput.focus();
    titleInput.select();
  });

  function changed() {
    onChange(note, title, content);
  }

  function selectImages(event: Event) {
    const files = Array.from((event.currentTarget as HTMLInputElement).files ?? []);
    if (files.length > 0) void onAddImages(files);
    (event.currentTarget as HTMLInputElement).value = "";
  }

  function selectAttachments(event: Event) {
    const files = Array.from((event.currentTarget as HTMLInputElement).files ?? []);
    if (files.length > 0) void onAddFiles(files);
    (event.currentTarget as HTMLInputElement).value = "";
  }

  function pasteImages(event: ClipboardEvent) {
    const files = Array.from(event.clipboardData?.files ?? []).filter((file) => file.type.startsWith("image/"));
    if (files.length === 0) return;
    event.preventDefault();
    void onAddImages(files);
  }

  function dropAttachments(event: DragEvent) {
    event.preventDefault();
    const files = Array.from(event.dataTransfer?.files ?? []);
    const images = files.filter((file) => file.type.startsWith("image/"));
    const ordinaryFiles = files.filter((file) => !file.type.startsWith("image/") && isSupportedFile(file.name));
    if (images.length > 0) void onAddImages(images);
    if (ordinaryFiles.length > 0) void onAddFiles(ordinaryFiles);
  }

  async function openFile(attachment: NoteAttachment) {
    fileActionError = "";
    try {
      await onOpenFile(attachment);
    } catch (reason) {
      fileActionError = reason instanceof Error ? reason.message : String(reason);
    }
  }

  async function saveAttachment(attachment: NoteAttachment) {
    fileActionError = "";
    try {
      const url = await onOpenAttachment(attachment);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = attachment.display_name;
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 0);
    } catch (reason) {
      fileActionError = reason instanceof Error ? reason.message : String(reason);
    }
  }

  function isSupportedFile(name: string) {
    return /\.(pdf|txt|md|markdown|docx|xlsx|pptx|zip)$/i.test(name);
  }

  function attachmentIndex(attachment: NoteAttachment) {
    return attachments.findIndex((item) => item.uuid === attachment.uuid);
  }

  function fileKind(attachment: NoteAttachment) {
    const extension = attachment.display_name.split(".").pop()?.toUpperCase();
    return extension && extension.length <= 8 ? extension : "FILE";
  }

  function formatBytes(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
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

  function handleViewerKeydown(event: KeyboardEvent) {
    if (viewerAttachment && event.key === "Escape") {
      event.stopPropagation();
      closeViewer();
    } else if (attachmentManagerOpen && event.key === "Escape") {
      event.stopPropagation();
      attachmentManagerOpen = false;
    }
  }

  function attachmentSummary() {
    if (imageAttachments.length > 0 && fileAttachments.length > 0) {
      return `${imageAttachments.length} 张图片 · ${fileAttachments.length} 个文件`;
    }
    if (imageAttachments.length > 0) return `${imageAttachments.length} 张图片`;
    return `${fileAttachments.length} 个文件`;
  }

  function attachmentState(attachment: NoteAttachment) {
    if (attachment.transfer_state === "failed") return "处理失败";
    if (attachment.transfer_state === "downloading") return "下载中";
    if (attachment.transfer_state === "cached") return "已缓存";
    if (attachment.transfer_state === "uploading") return "上传中";
    if (attachment.transfer_state === "pending_upload") return "等待同步";
    if (attachment.transfer_state === "remote_only") return "需要下载";
    return "已同步";
  }
</script>

<svelte:window onkeydown={handleViewerKeydown} />

<section
  class="note-editor"
  data-note-color={note.color}
  aria-label="编辑便签"
  ondragover={(event) => event.preventDefault()}
  ondrop={dropAttachments}
>
  <header>
    <button type="button" onclick={() => void onDone()}>返回</button>
    <span>{error ? error : saving ? "保存中…" : draft ? "开始输入后自动保存" : "已保存在本机"}</span>
    <input bind:this={imageInput} class="note-file-input" type="file" accept="image/jpeg,image/png,image/webp" multiple onchange={selectImages} />
    <input bind:this={attachmentInput} class="note-file-input" type="file" accept=".pdf,.txt,.md,.markdown,.docx,.xlsx,.pptx,.zip" multiple onchange={selectAttachments} />
    <div class="note-add-control">
      <button
        class="attachment-trigger"
        type="button"
        aria-expanded={addMenuOpen}
        disabled={attachmentBusy}
        onclick={() => (addMenuOpen = !addMenuOpen)}
      >添加</button>
      {#if addMenuOpen}
        <div class="note-add-menu">
          <button type="button" onclick={() => { addMenuOpen = false; imageInput.click(); }}>添加图片</button>
          <button type="button" onclick={() => { addMenuOpen = false; attachmentInput.click(); }}>添加文件</button>
        </div>
      {/if}
    </div>
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
    <section class="note-attachment-summary" aria-label="便签附件摘要">
      <button class="note-attachment-summary-heading" type="button" onclick={() => (attachmentManagerOpen = true)}>
        <span><strong>附件 {attachments.length}</strong><small>{attachmentSummary()}</small></span>
        <em>管理</em>
      </button>
      <div class="note-attachment-strip">
        {#each attachments as attachment (attachment.uuid)}
          <button type="button" title={attachment.display_name} onclick={() => (attachmentManagerOpen = true)}>
            {#if attachment.kind === "image" && attachmentPreviewUrls[attachment.uuid]}
              <img src={attachmentPreviewUrls[attachment.uuid]} alt="" aria-hidden="true" />
            {:else}
              <span>{attachment.kind === "image" ? "图片" : fileKind(attachment)}</span>
              {#if attachment.kind === "file"}<small>{attachment.display_name}</small>{/if}
            {/if}
            {#if attachment.transfer_state === "failed"}<em>!</em>{/if}
          </button>
        {/each}
      </div>
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

{#if attachmentManagerOpen}
  <div
    class="note-attachment-manager"
    role="dialog"
    aria-modal="true"
    aria-label="管理便签附件"
    tabindex="-1"
    onkeydown={handleViewerKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) attachmentManagerOpen = false;
    }}
  >
    <div data-note-color={note.color}>
      <header>
        <span><strong>管理附件</strong><small>{attachmentSummary()}</small></span>
        <button type="button" disabled={attachmentBusy} onclick={() => imageInput.click()}>添加图片</button>
        <button type="button" disabled={attachmentBusy} onclick={() => attachmentInput.click()}>添加文件</button>
        <button class="manager-close" type="button" aria-label="关闭附件管理" onclick={() => (attachmentManagerOpen = false)}>×</button>
      </header>
      <div class="note-attachment-manager-scroll">
        {#if imageAttachments.length > 0}
          <section>
            <h3>图片</h3>
            <div class="note-attachment-grid" aria-label="便签图片">
              {#each imageAttachments as attachment (attachment.uuid)}
                {@const index = attachmentIndex(attachment)}
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
                      <button class="attachment-order-button" type="button" title="向前移动" aria-label="向前移动" disabled={attachmentBusy || index <= 0} onclick={() => void onMoveAttachment(attachment, -1)}>←</button>
                      <button class="attachment-order-button" type="button" title="向后移动" aria-label="向后移动" disabled={attachmentBusy || index >= attachments.length - 1} onclick={() => void onMoveAttachment(attachment, 1)}>→</button>
                    {/if}
                    <button class="danger" type="button" disabled={attachmentBusy} onclick={() => void onDeleteAttachment(attachment)}>删除</button>
                  </div>
                </article>
              {/each}
            </div>
          </section>
        {/if}
        {#if fileAttachments.length > 0}
          <section>
            <h3>文件</h3>
            <div class="note-file-list" aria-label="便签附件">
              {#each fileAttachments as attachment (attachment.uuid)}
                {@const index = attachmentIndex(attachment)}
                <article class:failed={attachment.transfer_state === "failed"}>
                  <span class="note-file-kind" aria-hidden="true">{fileKind(attachment)}</span>
                  <div class="note-file-info">
                    <strong title={attachment.display_name}>{attachment.display_name}</strong>
                    <small title={attachment.transfer_error ?? attachment.display_name}>{formatBytes(attachment.byte_size)} · {attachmentState(attachment)}</small>
                  </div>
                  {#if attachment.transfer_state === "failed"}
                    <button type="button" disabled={attachmentBusy} onclick={() => void onRetryAttachment(attachment)}>重试</button>
                  {:else}
                    <button type="button" disabled={attachmentBusy} onclick={() => void openFile(attachment)}>打开</button>
                    <button type="button" disabled={attachmentBusy} onclick={() => void saveAttachment(attachment)}>保存</button>
                  {/if}
                  <button class="attachment-order-button" type="button" title="向前移动" aria-label="向前移动" disabled={attachmentBusy || index <= 0} onclick={() => void onMoveAttachment(attachment, -1)}>←</button>
                  <button class="attachment-order-button" type="button" title="向后移动" aria-label="向后移动" disabled={attachmentBusy || index >= attachments.length - 1} onclick={() => void onMoveAttachment(attachment, 1)}>→</button>
                  <button class="danger" type="button" disabled={attachmentBusy} onclick={() => void onDeleteAttachment(attachment)}>删除</button>
                </article>
              {/each}
              {#if fileActionError}<p class="note-file-error">{fileActionError}</p>{/if}
            </div>
          </section>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if viewerAttachment}
  <div
    class="note-image-viewer"
    role="dialog"
    aria-modal="true"
    aria-label="查看图片"
    tabindex="-1"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeViewer();
    }}
    onkeydown={handleViewerKeydown}
  >
    <div>
      <header>
        <strong>{viewerAttachment.display_name}</strong>
        <button class="viewer-close" type="button" title="关闭" aria-label="关闭图片查看器" onclick={closeViewer}>×</button>
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
