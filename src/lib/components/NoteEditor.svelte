<script lang="ts">
  import { onMount } from "svelte";
  import { languageState, translator, type TranslationKey } from "$lib/i18n";
  import { formatFileSize } from "$lib/i18n/formatters";
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

  function attachmentKindIndex(attachment: NoteAttachment) {
    const sameKind = attachment.kind === "image" ? imageAttachments : fileAttachments;
    return sameKind.findIndex((item) => item.uuid === attachment.uuid);
  }

  function attachmentKindCount(attachment: NoteAttachment) {
    return attachment.kind === "image" ? imageAttachments.length : fileAttachments.length;
  }

  function fileKind(attachment: NoteAttachment) {
    const extension = attachment.display_name.split(".").pop()?.toUpperCase();
    return extension && extension.length <= 8 ? extension : "FILE";
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
      return `${$translator("note.imagesCount", { count: imageAttachments.length })} · ${$translator("note.filesCount", { count: fileAttachments.length })}`;
    }
    if (imageAttachments.length > 0) return $translator("note.imagesCount", { count: imageAttachments.length });
    return $translator("note.filesCount", { count: fileAttachments.length });
  }

  function attachmentState(attachment: NoteAttachment) {
    if (attachment.transfer_state === "failed") return $translator("attachment.failed");
    if (attachment.transfer_state === "downloading") return $translator("attachment.downloading");
    if (attachment.transfer_state === "cached") return $translator("attachment.cached");
    if (attachment.transfer_state === "uploading") return $translator("attachment.uploading");
    if (attachment.transfer_state === "pending_upload") return $translator("attachment.pendingUpload");
    if (attachment.transfer_state === "remote_only") return $translator("attachment.needsDownload");
    return $translator("attachment.synced");
  }

  function colorName(color: NoteColor) {
    return $translator(`note.color${color === "default" ? "Default" : color[0].toUpperCase() + color.slice(1)}` as TranslationKey);
  }
</script>

<svelte:window onkeydown={handleViewerKeydown} />

<section
  class="note-editor"
  data-note-color={note.color}
  aria-label={$translator("note.edit")}
  ondragover={(event) => event.preventDefault()}
  ondrop={dropAttachments}
>
  <header>
    <button type="button" onclick={() => void onDone()}>{$translator("common.back")}</button>
    <span>{error ? error : saving ? $translator("note.saving") : draft ? $translator("note.autoSaveHint") : $translator("note.savedLocal")}</span>
    <input bind:this={imageInput} class="note-file-input" type="file" accept="image/jpeg,image/png,image/webp" multiple onchange={selectImages} />
    <input bind:this={attachmentInput} class="note-file-input" type="file" accept=".pdf,.txt,.md,.markdown,.docx,.xlsx,.pptx,.zip" multiple onchange={selectAttachments} />
    <div class="note-add-control">
      <button
        class="attachment-trigger"
        type="button"
        aria-expanded={addMenuOpen}
        disabled={attachmentBusy}
        onclick={() => (addMenuOpen = !addMenuOpen)}
      >{$translator("attachment.add")}</button>
      {#if addMenuOpen}
        <div class="note-add-menu">
          <button type="button" onclick={() => { addMenuOpen = false; imageInput.click(); }}>{$translator("attachment.addImage")}</button>
          <button type="button" onclick={() => { addMenuOpen = false; attachmentInput.click(); }}>{$translator("attachment.addFile")}</button>
        </div>
      {/if}
    </div>
    <button class="primary" type="button" onclick={() => void onDone()}>{$translator("common.done")}</button>
  </header>
  <input
    bind:this={titleInput}
    bind:value={title}
    maxlength="100"
    placeholder={$translator("note.titlePlaceholder")}
    aria-label={$translator("note.titlePlaceholder")}
    autocomplete="off"
    oninput={changed}
  />
  <textarea
    bind:value={content}
    maxlength="20000"
    placeholder={$translator("note.contentPlaceholder")}
    aria-label={$translator("note.contentPlaceholder")}
    autocomplete="off"
    oninput={changed}
    onpaste={pasteImages}
  ></textarea>
  {#if attachments.length > 0}
    <section class="note-attachment-summary" aria-label={$translator("note.attachments")}>
      <button class="note-attachment-summary-heading" type="button" onclick={() => (attachmentManagerOpen = true)}>
        <span><strong>{$translator("attachment.summary", { count: attachments.length })}</strong><small>{attachmentSummary()}</small></span>
        <em>{$translator("common.manage")}</em>
      </button>
      <div class="note-attachment-strip">
        {#each attachments as attachment (attachment.uuid)}
          <button type="button" title={attachment.display_name} onclick={() => (attachmentManagerOpen = true)}>
            {#if attachment.kind === "image" && attachmentPreviewUrls[attachment.uuid]}
              <img src={attachmentPreviewUrls[attachment.uuid]} alt="" aria-hidden="true" />
            {:else}
              <span>{attachment.kind === "image" ? $translator("attachment.image") : fileKind(attachment)}</span>
              {#if attachment.kind === "file"}<small>{attachment.display_name}</small>{/if}
            {/if}
            {#if attachment.transfer_state === "failed"}<em>!</em>{/if}
          </button>
        {/each}
      </div>
    </section>
  {/if}
  <footer>
    <div class="note-color-picker" aria-label={$translator("note.color")}>
      {#each colors as color}
        <button
          class:active={note.color === color}
          data-note-color={color}
          type="button"
          aria-label={$translator("note.changeToColor", { color: colorName(color) })}
          onclick={() => void onColor(note, color)}
        ></button>
      {/each}
    </div>
    <button type="button" onclick={() => void onPin(note, !note.pinned)}>{note.pinned ? $translator("note.unpin") : $translator("note.pin")}</button>
    <button class="danger" type="button" onclick={() => void onDelete(note)}>{draft ? $translator("note.discard") : $translator("common.delete")}</button>
  </footer>
</section>

{#if attachmentManagerOpen}
  <div
    class="note-attachment-manager"
    role="dialog"
    aria-modal="true"
    aria-label={$translator("attachment.managerLabel")}
    tabindex="-1"
    onkeydown={handleViewerKeydown}
    onclick={(event) => {
      if (event.target === event.currentTarget) attachmentManagerOpen = false;
    }}
  >
    <div data-note-color={note.color}>
      <header>
        <span><strong>{$translator("attachment.manage")}</strong><small>{attachmentSummary()}</small></span>
        <button type="button" disabled={attachmentBusy} onclick={() => imageInput.click()}>{$translator("attachment.addImage")}</button>
        <button type="button" disabled={attachmentBusy} onclick={() => attachmentInput.click()}>{$translator("attachment.addFile")}</button>
        <button class="manager-close" type="button" aria-label={$translator("attachment.closeManager")} onclick={() => (attachmentManagerOpen = false)}>×</button>
      </header>
      <div class="note-attachment-manager-scroll">
        {#if imageAttachments.length > 0}
          <section>
            <h3>{$translator("attachment.image")}</h3>
            <div class="note-attachment-grid" aria-label={$translator("attachment.images")}>
              {#each imageAttachments as attachment (attachment.uuid)}
                {@const index = attachmentKindIndex(attachment)}
                <article class:failed={attachment.transfer_state === "failed"}>
                  <button class="note-attachment-preview" type="button" onclick={() => void openAttachment(attachment)}>
                    {#if attachmentPreviewUrls[attachment.uuid]}
                      <img src={attachmentPreviewUrls[attachment.uuid]} alt={attachment.display_name} />
                    {:else}
                      <span>{attachment.transfer_state === "remote_only" ? $translator("attachment.needsPreview") : $translator("attachment.preparing")}</span>
                    {/if}
                  </button>
                  <div>
                    <small title={attachment.transfer_error ?? attachment.display_name}>{attachmentState(attachment)}</small>
                    {#if attachment.transfer_state === "failed"}
                      <button type="button" disabled={attachmentBusy} onclick={() => void onRetryAttachment(attachment)}>{$translator("attachment.retry")}</button>
                    {:else}
                      <button class="attachment-order-button" type="button" title={$translator("attachment.moveForward")} aria-label={$translator("attachment.moveForward")} disabled={attachmentBusy || index <= 0} onclick={() => void onMoveAttachment(attachment, -1)}>←</button>
                      <button class="attachment-order-button" type="button" title={$translator("attachment.moveBackward")} aria-label={$translator("attachment.moveBackward")} disabled={attachmentBusy || index >= attachmentKindCount(attachment) - 1} onclick={() => void onMoveAttachment(attachment, 1)}>→</button>
                    {/if}
                    <button class="danger" type="button" disabled={attachmentBusy} onclick={() => void onDeleteAttachment(attachment)}>{$translator("common.delete")}</button>
                  </div>
                </article>
              {/each}
            </div>
          </section>
        {/if}
        {#if fileAttachments.length > 0}
          <section>
            <h3>{$translator("attachment.file")}</h3>
            <div class="note-file-list" aria-label={$translator("note.attachments")}>
              {#each fileAttachments as attachment (attachment.uuid)}
                {@const index = attachmentKindIndex(attachment)}
                <article class:failed={attachment.transfer_state === "failed"}>
                  <span class="note-file-kind" aria-hidden="true">{fileKind(attachment)}</span>
                  <div class="note-file-info">
                    <strong title={attachment.display_name}>{attachment.display_name}</strong>
                    <small title={attachment.transfer_error ?? attachment.display_name}>{formatFileSize(attachment.byte_size, $languageState.resolvedLocale)} · {attachmentState(attachment)}</small>
                  </div>
                  {#if attachment.transfer_state === "failed"}
                    <button type="button" disabled={attachmentBusy} onclick={() => void onRetryAttachment(attachment)}>{$translator("attachment.retry")}</button>
                  {:else}
                    <button type="button" disabled={attachmentBusy} onclick={() => void openFile(attachment)}>{$translator("attachment.open")}</button>
                    <button type="button" disabled={attachmentBusy} onclick={() => void saveAttachment(attachment)}>{$translator("attachment.save")}</button>
                  {/if}
                  <button class="attachment-order-button" type="button" title={$translator("attachment.moveForward")} aria-label={$translator("attachment.moveForward")} disabled={attachmentBusy || index <= 0} onclick={() => void onMoveAttachment(attachment, -1)}>←</button>
                  <button class="attachment-order-button" type="button" title={$translator("attachment.moveBackward")} aria-label={$translator("attachment.moveBackward")} disabled={attachmentBusy || index >= attachmentKindCount(attachment) - 1} onclick={() => void onMoveAttachment(attachment, 1)}>→</button>
                  <button class="danger" type="button" disabled={attachmentBusy} onclick={() => void onDeleteAttachment(attachment)}>{$translator("common.delete")}</button>
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
    aria-label={$translator("attachment.viewer")}
    tabindex="-1"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeViewer();
    }}
    onkeydown={handleViewerKeydown}
  >
    <div>
      <header>
        <strong>{viewerAttachment.display_name}</strong>
        <button class="viewer-close" type="button" title={$translator("common.close")} aria-label={$translator("attachment.viewerClose")} onclick={closeViewer}>×</button>
      </header>
      {#if viewerLoading}
        <span>{$translator("attachment.loadingOriginal")}</span>
      {:else if viewerUrl}
        <img src={viewerUrl} alt={viewerAttachment.display_name} />
        <a href={viewerUrl} download={viewerAttachment.display_name}>{$translator("attachment.saveOriginal")}</a>
      {:else}
        <span>{viewerError || $translator("attachment.viewerUnavailable")}</span>
      {/if}
    </div>
  </div>
{/if}
