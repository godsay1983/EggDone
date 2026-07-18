<script lang="ts">
  import { onMount } from "svelte";

  import {
    dataApi,
    type ImportPreview,
    type ImportResult,
  } from "$lib/api/dataApi";
  import { formatFileSize } from "$lib/i18n/formatters";
  import { languageState, translator } from "$lib/i18n";

  export let onClose: () => void;
  export let onImported: () => Promise<void>;

  let busy = false;
  let preview: ImportPreview | null = null;
  let importKind: "json" | "backup" | null = null;
  let message = "";
  let error = "";
  let dialogElement: HTMLDivElement;

  onMount(() => dialogElement?.focus());

  async function exportData() {
    await runAction(async () => {
      const path = await dataApi.exportTodos();
      if (path) message = $translator("data.exportJsonSuccess");
    });
  }

  async function chooseImport() {
    await runAction(async () => {
      preview = await dataApi.previewImport();
      if (preview) {
        importKind = "json";
        message = "";
      }
    });
  }

  async function chooseFullBackupImport() {
    await runAction(async () => {
      preview = await dataApi.previewFullBackupImport();
      if (preview) {
        importKind = "backup";
        message = "";
      }
    });
  }

  async function confirmImport() {
    if (!preview) return;
    await runAction(async () => {
      const result = importKind === "backup"
        ? await dataApi.confirmFullBackupImport(preview!.path)
        : await dataApi.confirmImport(preview!.path);
      preview = null;
      importKind = null;
      message = importMessage(result);
      await onImported();
    });
  }

  async function backupDatabase() {
    await runAction(async () => {
      const path = await dataApi.backupDatabase();
      if (path) message = $translator("data.backupSqliteSuccess");
    });
  }

  async function exportFullBackup() {
    await runAction(async () => {
      const result = await dataApi.exportFullBackup();
      if (result) {
        message = $translator("data.exportFullBackupSuccess", {
          attachments: result.attachment_count,
          files: result.file_count,
        });
      }
    });
  }

  async function runAction(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await action();
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    } finally {
      busy = false;
    }
  }

  function importMessage(result: ImportResult) {
    return $translator("data.importComplete", {
      taskAdded: result.added,
      taskUpdated: result.updated,
      taskUnchanged: result.unchanged,
      noteAdded: result.note_added,
      noteUpdated: result.note_updated,
      noteUnchanged: result.note_unchanged,
      attachmentAdded: result.attachment_added,
      attachmentUpdated: result.attachment_updated,
      attachmentUnchanged: result.attachment_unchanged,
      restoredFiles: result.restored_file_count,
    });
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape" && !busy) onClose();
  }}
/>

<div class="data-backdrop">
  <button
    class="data-dismiss"
    type="button"
    aria-label={$translator("data.close")}
    onclick={onClose}
  ></button>
  <div
    bind:this={dialogElement}
    class="data-card"
    role="dialog"
    aria-modal="true"
    aria-labelledby="data-title"
    tabindex="-1"
  >
    <header>
      <div>
        <h2 id="data-title">{$translator("data.title")}</h2>
        <p>{$translator("data.subtitle")}</p>
      </div>
      <button type="button" aria-label={$translator("data.close")} onclick={onClose}>×</button>
    </header>

    <div class="data-actions">
      <button type="button" disabled={busy} onclick={() => void exportData()}>
        <strong>{$translator("data.exportJson")}</strong>
        <span>{$translator("data.exportJsonHelp")}</span>
      </button>
      <button type="button" disabled={busy} onclick={() => void chooseImport()}>
        <strong>{$translator("data.importJson")}</strong>
        <span>{$translator("data.importJsonHelp")}</span>
      </button>
      <button type="button" disabled={busy} onclick={() => void backupDatabase()}>
        <strong>{$translator("data.backupSqlite")}</strong>
        <span>{$translator("data.backupSqliteHelp")}</span>
      </button>
      <button type="button" disabled={busy} onclick={() => void exportFullBackup()}>
        <strong>{$translator("data.exportFullBackup")}</strong>
        <span>{$translator("data.exportFullBackupHelp")}</span>
      </button>
      <button type="button" disabled={busy} onclick={() => void chooseFullBackupImport()}>
        <strong>{$translator("data.importFullBackup")}</strong>
        <span>{$translator("data.importFullBackupHelp")}</span>
      </button>
    </div>

    {#if preview}
      <div class="import-preview">
        <strong>{$translator("data.confirmImportFile", { name: preview.file_name })}</strong>
        <span>{$translator("data.previewSummary", { tasks: preview.total, notes: preview.note_total, attachments: preview.attachment_total })}</span>
        <div>
          <span>{$translator("data.taskChanges", { added: preview.added, updated: preview.updated, unchanged: preview.unchanged })}</span>
          <span>{$translator("data.noteChanges", { added: preview.note_added, updated: preview.note_updated, unchanged: preview.note_unchanged })}</span>
          <span>{$translator("data.attachmentChanges", { added: preview.attachment_added, updated: preview.attachment_updated, unchanged: preview.attachment_unchanged })}</span>
          {#if preview.attachment_files_included}
            <span>{$translator("data.backupValidated", { files: preview.backup_file_count, size: formatFileSize(preview.backup_total_bytes, $languageState.resolvedLocale) })}</span>
          {:else}
            <span>{$translator("data.jsonAttachmentsHelp")}</span>
          {/if}
        </div>
        <div class="preview-actions">
          <button type="button" disabled={busy} onclick={() => { preview = null; importKind = null; }}>
            {$translator("common.cancel")}
          </button>
          <button type="button" disabled={busy} onclick={() => void confirmImport()}>
            {$translator("data.confirmMerge")}
          </button>
        </div>
      </div>
    {/if}

    {#if message}<p class="data-message" role="status">{message}</p>{/if}
    {#if error}<p class="data-error" role="alert">{error}</p>{/if}
  </div>
</div>
