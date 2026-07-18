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
  import { translator } from "$lib/i18n";
  import { formatFileSize, formatTime } from "$lib/i18n/formatters";

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
    if (!window.confirm($translator("sync.cacheClearConfirm"))) return;
    cacheBusy = true;
    cacheError = "";
    cacheMessage = "";
    const removedBytes = cacheStats.reclaimableBytes;
    try {
      cacheStats = await noteAttachmentApi.clearCache();
      cacheMessage = $translator("sync.cacheCleared", {
        size: formatFileSize(removedBytes),
      });
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
      message = $translator("sync.settingsSaved");
      if (testAfterSave) {
        const result = await testSyncConnection();
        message = localizedSyncMessage(result.message);
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
    message = $translator("sync.downloadingAndMerging");
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
      message = $translator("sync.resultSummary", {
        message: localizedSyncMessage(result.message),
        todos: result.todoCount,
        notes: result.noteCount,
      });
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
      message = $translator("sync.credentialsDeleted");
    } catch (reason) {
      error = errorMessage(reason);
    } finally {
      busy = false;
    }
  }

  function errorMessage(reason: unknown) {
    return localizedSyncMessage(reason instanceof Error ? reason.message : String(reason));
  }

  function localizedSyncMessage(raw: string) {
    if (raw === "连接成功，已找到同步文件") return $translator("sync.connectionFound");
    if (raw === "连接成功，同步文件尚未创建") return $translator("sync.connectionMissing");
    if (raw === "任务、便签和附件同步完成") return $translator("sync.serviceCompleted");
    if (raw === "检测到远端更新，重新合并后同步完成") {
      return $translator("sync.serviceConflictMerged");
    }
    if (raw === "同步未启用") return $translator("sync.disabledStatus");
    if (raw === "同步凭据未配置") return $translator("sync.credentialsMissing");
    if (raw === "正在同步…") return $translator("sync.syncing");
    if (raw === "网络暂时不可用，将在下次同步时重试") {
      return $translator("sync.offlineAdvice");
    }
    if (raw === "远端内容持续变化，请稍后再次同步" || raw.includes("远端文件持续发生变化")) {
      return $translator("sync.conflictAdvice");
    }
    if (raw === "同步未完成，请重试") return $translator("sync.retryAdvice");
    const retry = raw.match(/^网络异常，正在第 (\d+) 次重试…$/);
    if (retry) return $translator("sync.networkRetry", { attempt: retry[1] });
    const completed = raw.match(/^同步完成：任务 (\d+)，便签 (\d+)，附件 (\d+)$/);
    if (completed) {
      return $translator("sync.completedCounts", {
        todos: completed[1],
        notes: completed[2],
        attachments: completed[3],
      });
    }
    const merged = raw.match(/^冲突已合并：任务 (\d+)，便签 (\d+)，附件 (\d+)$/);
    if (merged) {
      return $translator("sync.conflictMergedCounts", {
        todos: merged[1],
        notes: merged[2],
        attachments: merged[3],
      });
    }
    if (raw.includes("远端附件清理未完成")) {
      return $translator("sync.remoteCleanupWarning");
    }
    return raw;
  }
</script>

<section class="sync-section" aria-labelledby="sync-title">
  <div class="sync-heading">
    <div>
      <strong id="sync-title">{$translator("sync.title")}</strong>
      <span>{$translator("sync.subtitle")}</span>
    </div>
    {#if settings}
      <label class="switch">
        <input
          type="checkbox"
          bind:checked={settings.enabled}
          disabled={busy}
          aria-label={$translator("sync.enable")}
        />
        <span></span>
      </label>
    {/if}
  </div>

  {#if settings?.enabled}
    <p
      class:status-error={["offline", "conflict", "failed"].includes($syncStatus.kind)}
      class="sync-status"
      title={localizedSyncMessage($syncStatus.detail ?? $syncStatus.message)}
    >
      {localizedSyncMessage($syncStatus.message)}
      {#if $syncStatus.updatedAt}
        <small>{formatTime($syncStatus.updatedAt)}</small>
      {/if}
    </p>
  {/if}

  {#if !settings}
    <p class="sync-placeholder">{busy ? $translator("sync.loadingSettings") : $translator("sync.settingsUnavailable")}</p>
  {:else}
    <label class="sync-field">
      <span>Endpoint <small>{$translator("sync.endpointHelp")}</small></span>
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
        <span>{$translator("sync.httpRiskConfirm")}</span>
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
      <span>{$translator("sync.todoObjectKey")}</span>
      <input bind:value={settings.objectKey} disabled={busy} />
    </label>

    <label class="sync-field">
      <span>{$translator("sync.noteObjectKey")} <small>{$translator("sync.noteObjectKeyHelp")}</small></span>
      <input value={settings.noteObjectKey} readonly aria-readonly="true" />
    </label>

    <label class="sync-field">
      <span>{$translator("sync.attachmentMetadataKey")} <small>{$translator("sync.readOnlyShared")}</small></span>
      <input value={settings.noteAttachmentObjectKey} readonly aria-readonly="true" />
    </label>

    <label class="sync-field">
      <span>{$translator("sync.assetPrefix")} <small>{$translator("sync.assetPrefixHelp")}</small></span>
      <input value={settings.noteAssetPrefix} readonly aria-readonly="true" />
    </label>

    <label class="path-style">
      <input type="checkbox" bind:checked={settings.pathStyle} disabled={busy} />
      <span>{$translator("sync.pathStyleHelp")}</span>
    </label>

    <div class="credential-status">
      <span>{$translator("sync.credentials")}</span>
      <strong class:configured={settings.credentialsConfigured}>
        {settings.credentialsConfigured ? $translator("sync.saved") : $translator("sync.notSaved")}
      </strong>
    </div>

    <label class="sync-field">
      <span>Access Key <small>{$translator("sync.keepSavedAccessKey")}</small></span>
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
        {$translator("common.save")}
      </button>
      <button
        type="button"
        disabled={busy}
        onclick={() => void save(true)}
      >
        {$translator("sync.testConnection")}
      </button>
      <button
        class="primary"
        type="button"
        disabled={busy || !settings.enabled}
        onclick={() => void synchronize()}
      >
        {busy ? $translator("common.processing") : $translator("sync.manual")}
      </button>
    </div>

    {#if settings.credentialsConfigured}
      <button
        class="delete-credentials"
        type="button"
        disabled={busy}
        onclick={() => void removeCredentials()}
      >
        {$translator("sync.deleteCredentials")}
      </button>
    {/if}
  {/if}

  <section class="attachment-cache" aria-labelledby="attachment-cache-title">
    <div class="attachment-cache-heading">
      <div>
        <strong id="attachment-cache-title">{$translator("sync.attachmentCache")}</strong>
        <span>{$translator("sync.attachmentCacheHelp")}</span>
      </div>
      <button
        type="button"
        disabled={cacheBusy || !cacheStats || cacheStats.reclaimableBytes === 0}
        onclick={() => void clearAttachmentCache()}
      >
        {cacheBusy ? $translator("sync.clearingCache") : $translator("sync.clearCache")}
      </button>
    </div>
    {#if cacheStats}
      <div class="attachment-sync-pending">
        <span>{$translator("sync.pendingAttachments")}</span>
        <strong>{$translator("sync.attachmentCount", { count: cacheStats.pendingCount })}</strong>
      </div>
      <div class="attachment-cache-stats">
        <span>{$translator("sync.localUsage")} <strong>{formatFileSize(cacheStats.totalBytes)}</strong></span>
        <span>{$translator("sync.reclaimable")} <strong>{formatFileSize(cacheStats.reclaimableBytes)}</strong></span>
        <span>{$translator("sync.pendingUploadProtected")} <strong>{formatFileSize(cacheStats.protectedBytes)}</strong></span>
      </div>
    {:else}
      <p class="sync-placeholder">{$translator("sync.calculatingCache")}</p>
    {/if}
    {#if cacheMessage}<p class="sync-message" role="status">{cacheMessage}</p>{/if}
    {#if cacheError}<p class="settings-error" role="alert">{cacheError}</p>{/if}
  </section>

  {#if message}<p class="sync-message" role="status">{message}</p>{/if}
  {#if error}<p class="settings-error" role="alert">{error}</p>{/if}
</section>
