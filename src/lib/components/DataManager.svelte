<script lang="ts">
  import { onMount } from "svelte";

  import {
    dataApi,
    type ImportPreview,
    type ImportResult,
  } from "$lib/api/dataApi";

  export let onClose: () => void;
  export let onImported: () => Promise<void>;

  let busy = false;
  let preview: ImportPreview | null = null;
  let message = "";
  let error = "";
  let dialogElement: HTMLDivElement;

  onMount(() => dialogElement?.focus());

  async function exportTodos() {
    await runAction(async () => {
      const path = await dataApi.exportTodos();
      if (path) message = "Todo JSON 已导出";
    });
  }

  async function chooseImport() {
    await runAction(async () => {
      preview = await dataApi.previewImport();
      if (preview) message = "";
    });
  }

  async function confirmImport() {
    if (!preview) return;
    await runAction(async () => {
      const result = await dataApi.confirmImport(preview!.path);
      preview = null;
      message = importMessage(result);
      await onImported();
    });
  }

  async function backupDatabase() {
    await runAction(async () => {
      const path = await dataApi.backupDatabase();
      if (path) message = "SQLite 数据库已备份";
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
    return `导入完成：新增 ${result.added}，更新 ${result.updated}，保持 ${result.unchanged}`;
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
    aria-label="关闭数据管理"
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
        <h2 id="data-title">数据管理</h2>
        <p>导出、恢复或备份本地任务</p>
      </div>
      <button type="button" aria-label="关闭数据管理" onclick={onClose}>×</button>
    </header>

    <div class="data-actions">
      <button type="button" disabled={busy} onclick={() => void exportTodos()}>
        <strong>导出 Todo JSON</strong>
        <span>用于交换数据或迁移设备</span>
      </button>
      <button type="button" disabled={busy} onclick={() => void chooseImport()}>
        <strong>导入 Todo JSON</strong>
        <span>按 UUID 合并，不覆盖较新的本地任务</span>
      </button>
      <button type="button" disabled={busy} onclick={() => void backupDatabase()}>
        <strong>备份 SQLite</strong>
        <span>保存完整数据库快照</span>
      </button>
    </div>

    {#if preview}
      <div class="import-preview">
        <strong>确认导入 {preview.file_name}？</strong>
        <span>共 {preview.total} 项</span>
        <div>
          <span>新增 {preview.added}</span>
          <span>更新 {preview.updated}</span>
          <span>保持 {preview.unchanged}</span>
        </div>
        <div class="preview-actions">
          <button type="button" disabled={busy} onclick={() => (preview = null)}>
            取消
          </button>
          <button type="button" disabled={busy} onclick={() => void confirmImport()}>
            确认合并
          </button>
        </div>
      </div>
    {/if}

    {#if message}<p class="data-message" role="status">{message}</p>{/if}
    {#if error}<p class="data-error" role="alert">{error}</p>{/if}
  </div>
</div>
