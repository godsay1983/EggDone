<script lang="ts">
  import { onMount } from "svelte";
  import {
    deleteSyncCredentials,
    getSyncSettings,
    saveSyncSettings,
    testSyncConnection,
    type SyncSettings,
  } from "$lib/api/syncApi";
  import {
    configureAutoSync,
    runManualSync,
    syncStatus,
  } from "$lib/sync/autoSync";
  import {
    noteAttachmentApi,
    type NoteAttachmentCacheStats,
  } from "$lib/api/noteAttachmentApi";

  let settings: SyncSettings | null = null;
  let accessKey = "";
  let secretKey = "";
  let busy = false;
  let error = "";
  let message = "";
  let cacheStats: NoteAttachmentCacheStats | null = null;
  let cacheBusy = false;
  let cacheMessage = "";
  let cacheError = "";

  $: usesHttp = settings?.endpoint.trim().toLowerCase().startsWith("http://") ?? false;

  onMount(() => {
    void load();
    void loadAttachmentCacheStats();
  });

  async function loadAttachmentCacheStats() {
    cacheError = "";
    try {
      cacheStats = await noteAttachmentApi.cacheStats();
    } catch (reason) {
      cacheError = errorMessage(reason);
    }
  }

  async function clearAttachmentCache() {
    if (cacheBusy || !cacheStats || cacheStats.reclaimableBytes === 0) return;
    if (!window.confirm("清理已同步附件的本地副本？需要时会从远端重新下载。")) return;
    cacheBusy = true;
    cacheError = "";
    cacheMessage = "";
    const removedBytes = cacheStats.reclaimableBytes;
    try {
      cacheStats = await noteAttachmentApi.clearCache();
      cacheMessage = `已清理 ${formatBytes(removedBytes)} 可重新下载缓存`;
    } catch (reason) {
      cacheError = errorMessage(reason);
    } finally {
      cacheBusy = false;
    }
  }

  async function load() {
    busy = true;
    error = "";
    try {
      settings = await getSyncSettings();
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      busy = false;
    }
  }

  async function save(testAfterSave = false) {
    if (!settings || busy) return;
    busy = true;
    error = "";
    message = "";
    try {
      settings = await saveSyncSettings({
        enabled: settings.enabled,
        endpoint: settings.endpoint,
        region: settings.region,
        bucket: settings.bucket,
        objectKey: settings.objectKey,
        pathStyle: settings.pathStyle,
        allowHttp: settings.allowHttp,
        accessKey: accessKey.trim() || null,
        secretKey: secretKey || null,
      });
      accessKey = "";
      secretKey = "";
      configureAutoSync(settings);
      message = "同步配置已保存";
      if (testAfterSave) {
        const result = await testSyncConnection();
        message = result.message;
      }
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      busy = false;
    }
  }

  async function synchronize() {
    if (!settings || busy) return;
    busy = true;
    error = "";
    message = "正在下载并合并远端任务和便签…";
    try {
      settings = await saveSyncSettings({
        enabled: settings.enabled,
        endpoint: settings.endpoint,
        region: settings.region,
        bucket: settings.bucket,
        objectKey: settings.objectKey,
        pathStyle: settings.pathStyle,
        allowHttp: settings.allowHttp,
        accessKey: accessKey.trim() || null,
        secretKey: secretKey || null,
      });
      accessKey = "";
      secretKey = "";
      configureAutoSync(settings);
      const result = await runManualSync();
      message = `${result.message}，任务 ${result.todoCount} 条，便签 ${result.noteCount} 条`;
      await loadAttachmentCacheStats();
    } catch (reason) {
      message = "";
      error = errorMessage(reason);
    } finally {
      busy = false;
    }
  }

  async function removeCredentials() {
    if (!settings || busy) return;
    busy = true;
    error = "";
    message = "";
    try {
      await deleteSyncCredentials();
      settings = { ...settings, enabled: false, credentialsConfigured: false };
      configureAutoSync(settings);
      message = "同步凭据已删除，同步已禁用";
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      busy = false;
    }
  }

  function errorMessage(reason: unknown) {
    return reason instanceof Error ? reason.message : String(reason);
  }

  function formatBytes(bytes: number) {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  }
</script>

<section class="sync-section" aria-labelledby="sync-title">
  <div class="sync-heading">
    <div>
      <strong id="sync-title">S3 / MinIO 同步</strong>
      <span>合并本地与远端任务、便签</span>
    </div>
    {#if settings}
      <label class="switch">
        <input
          type="checkbox"
          bind:checked={settings.enabled}
          disabled={busy}
          aria-label="启用同步"
        />
        <span></span>
      </label>
    {/if}
  </div>

  {#if settings?.enabled}
    <p class:status-error={["offline", "conflict", "failed"].includes($syncStatus.kind)} class="sync-status">
      {$syncStatus.message}
      {#if $syncStatus.updatedAt}
        <small>{new Date($syncStatus.updatedAt).toLocaleTimeString()}</small>
      {/if}
    </p>
  {/if}

  {#if !settings}
    <p class="sync-placeholder">{busy ? "正在读取同步配置…" : "无法读取同步配置"}</p>
  {:else}
    <label class="sync-field">
      <span>Endpoint <small>留空使用 AWS S3</small></span>
      <input
        type="url"
        bind:value={settings.endpoint}
        placeholder="http://127.0.0.1:9000"
        disabled={busy}
      />
    </label>

    {#if usesHttp}
      <label class="http-warning">
        <input type="checkbox" bind:checked={settings.allowHttp} disabled={busy} />
        <span>我了解 HTTP 会明文传输凭据、任务和便签数据</span>
      </label>
    {/if}

    <div class="sync-grid">
      <label class="sync-field">
        <span>Region</span>
        <input bind:value={settings.region} disabled={busy} />
      </label>
      <label class="sync-field">
        <span>Bucket</span>
        <input bind:value={settings.bucket} disabled={busy} />
      </label>
    </div>

    <label class="sync-field">
      <span>任务 Object Key</span>
      <input bind:value={settings.objectKey} disabled={busy} />
    </label>

    <label class="sync-field">
      <span>便签 Object Key <small>根据任务路径自动生成</small></span>
      <input value={settings.noteObjectKey} readonly aria-readonly="true" />
    </label>

    <label class="path-style">
      <input type="checkbox" bind:checked={settings.pathStyle} disabled={busy} />
      <span>使用 Path Style（MinIO 通常需要）</span>
    </label>

    <div class="credential-status">
      <span>系统凭据</span>
      <strong class:configured={settings.credentialsConfigured}>
        {settings.credentialsConfigured ? "已保存" : "未保存"}
      </strong>
    </div>

    <label class="sync-field">
      <span>Access Key <small>留空则保留已保存值</small></span>
      <input
        type="password"
        bind:value={accessKey}
        autocomplete="off"
        disabled={busy}
      />
    </label>
    <label class="sync-field">
      <span>Secret Key</span>
      <input
        type="password"
        bind:value={secretKey}
        autocomplete="off"
        disabled={busy}
      />
    </label>

    <div class="sync-actions">
      <button type="button" disabled={busy} onclick={() => void save(false)}>
        保存
      </button>
      <button
        type="button"
        disabled={busy}
        onclick={() => void save(true)}
      >
        测试连接
      </button>
      <button
        class="primary"
        type="button"
        disabled={busy || !settings.enabled}
        onclick={() => void synchronize()}
      >
        {busy ? "处理中…" : "立即同步"}
      </button>
    </div>

    {#if settings.credentialsConfigured}
      <button
        class="delete-credentials"
        type="button"
        disabled={busy}
        onclick={() => void removeCredentials()}
      >
        删除凭据并禁用同步
      </button>
    {/if}
  {/if}

  <section class="attachment-cache" aria-labelledby="attachment-cache-title">
    <div class="attachment-cache-heading">
      <div>
        <strong id="attachment-cache-title">附件缓存</strong>
        <span>只清理已同步、可从远端重新下载的本地副本</span>
      </div>
      <button
        type="button"
        disabled={cacheBusy || !cacheStats || cacheStats.reclaimableBytes === 0}
        onclick={() => void clearAttachmentCache()}
      >
        {cacheBusy ? "清理中…" : "清理缓存"}
      </button>
    </div>
    {#if cacheStats}
      <div class="attachment-cache-stats">
        <span>本地占用 <strong>{formatBytes(cacheStats.totalBytes)}</strong></span>
        <span>可清理 <strong>{formatBytes(cacheStats.reclaimableBytes)}</strong></span>
        <span>待上传保护 <strong>{formatBytes(cacheStats.protectedBytes)}</strong></span>
      </div>
    {:else}
      <p class="sync-placeholder">正在统计附件缓存…</p>
    {/if}
    {#if cacheMessage}<p class="sync-message" role="status">{cacheMessage}</p>{/if}
    {#if cacheError}<p class="settings-error" role="alert">{cacheError}</p>{/if}
  </section>

  {#if message}<p class="sync-message" role="status">{message}</p>{/if}
  {#if error}<p class="settings-error" role="alert">{error}</p>{/if}
</section>
