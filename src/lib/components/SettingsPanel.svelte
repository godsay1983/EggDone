<script lang="ts">
  import {
    shortcutOptions,
    updateAutostart,
    updateShortcut,
    type DesktopSettings,
  } from "$lib/api/desktopSettings";
  import SyncSettings from "./SyncSettings.svelte";

  export let settings: DesktopSettings;
  export let onClose: () => void;
  export let onChange: (settings: DesktopSettings) => void;

  let busy = false;
  let error = settings.shortcutError ?? settings.autostartError ?? "";

  async function setShortcutEnabled(enabled: boolean) {
    await saveShortcut(settings.shortcut, enabled);
  }

  async function setShortcut(shortcut: string) {
    await saveShortcut(shortcut, settings.shortcutEnabled);
  }

  async function saveShortcut(shortcut: string, enabled: boolean) {
    if (busy) return;
    busy = true;
    error = "";
    const previous = settings;
    try {
      await updateShortcut(
        previous.shortcut,
        previous.shortcutEnabled,
        shortcut,
        enabled,
      );
      onChange({
        ...settings,
        shortcut,
        shortcutEnabled: enabled,
        shortcutError: null,
      });
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      onChange({ ...previous, shortcutError: error });
    } finally {
      busy = false;
    }
  }

  async function setAutostart(enabled: boolean) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      const actual = await updateAutostart(enabled);
      onChange({
        ...settings,
        autostartEnabled: actual,
        autostartError: null,
      });
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === "Escape" && !busy) onClose();
  }}
/>

<div class="settings-backdrop">
  <button
    class="settings-dismiss"
    type="button"
    aria-label="关闭设置"
    onclick={onClose}
  ></button>
  <section class="settings-card" aria-labelledby="settings-title">
    <header>
      <div>
        <h2 id="settings-title">设置</h2>
        <p>桌面快捷操作</p>
      </div>
      <button type="button" aria-label="关闭设置" onclick={onClose}>×</button>
    </header>

    <div class="setting-row">
      <div>
        <strong>全局快捷键</strong>
        <span>快速打开面板并聚焦输入框</span>
      </div>
      <label class="switch">
        <input
          type="checkbox"
          checked={settings.shortcutEnabled}
          disabled={busy}
          onchange={(event) =>
            void setShortcutEnabled(event.currentTarget.checked)}
        />
        <span></span>
      </label>
    </div>

    <label class="shortcut-select">
      <span>快捷键组合</span>
      <select
        value={settings.shortcut}
        disabled={busy || !settings.shortcutEnabled}
        onchange={(event) => void setShortcut(event.currentTarget.value)}
      >
        {#each shortcutOptions as option}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>

    <div class="setting-row">
      <div>
        <strong>开机自动运行</strong>
        <span>启动后静默驻留系统托盘</span>
      </div>
      <label class="switch">
        <input
          type="checkbox"
          checked={settings.autostartEnabled}
          disabled={busy}
          onchange={(event) => void setAutostart(event.currentTarget.checked)}
        />
        <span></span>
      </label>
    </div>

    <SyncSettings />

    {#if error}<p class="settings-error" role="alert">{error}</p>{/if}
  </section>
</div>
