<script lang="ts">
  import { onMount } from "svelte";
  import {
    deleteSyncCredentials,
    getSyncSettings,
    saveSyncSettings,
    syncNow,
    testSyncConnection,
    type SyncSettings,
  } from "$lib/api/syncApi";

  let settings: SyncSettings | null = null;
  let accessKey = "";
  let secretKey = "";
  let busy = false;
  let error = "";
  let message = "";

  $: usesHttp = settings?.endpoint.trim().toLowerCase().startsWith("http://") ?? false;

  onMount(() => {
    void load();
  });

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
    message = "正在下载并合并远端任务…";
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
      const result = await syncNow();
      message = `${result.message}，共 ${result.todoCount} 条任务`;
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
</script>

<section class="sync-section" aria-labelledby="sync-title">
  <div class="sync-heading">
    <div>
      <strong id="sync-title">S3 / MinIO 同步</strong>
      <span>手动合并本地与远端任务</span>
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
        <span>我了解 HTTP 会明文传输凭据和 Todo 数据</span>
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
      <span>Object Key</span>
      <input bind:value={settings.objectKey} disabled={busy} />
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

  {#if message}<p class="sync-message" role="status">{message}</p>{/if}
  {#if error}<p class="settings-error" role="alert">{error}</p>{/if}
</section>
