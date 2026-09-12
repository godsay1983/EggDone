<script lang="ts">
  import {
    shortcutOptions,
    noteShortcutOptions,
    updateNoteShortcut,
    updateAutostart,
    updateShortcut,
    refreshDesktopSettings,
    type CapabilityStatus,
    type DesktopSettings,
  } from "$lib/api/desktopSettings";
  import {
    BREAK_DURATION_OPTIONS,
    FOCUS_DURATION_OPTIONS,
    getBreakDurationMinutes,
    getFocusDurationMinutes,
    saveBreakDurationMinutes,
    saveFocusDurationMinutes,
  } from "$lib/utils/focusSettings";
  import {
    languageState,
    setLanguageMode,
    translator,
    type LanguageMode,
    type TranslationKey,
  } from "$lib/i18n";
  import type { DefaultListViewMode } from "$lib/utils/viewPreferences";
  import { onMount, tick } from "svelte";
  import SyncSettings from "./SyncSettings.svelte";
  import WindowSettings from "./WindowSettings.svelte";
  import PreferenceStatus from './PreferenceStatus.svelte';

  export let settings: DesktopSettings;
  export let defaultListViewMode: DefaultListViewMode;
  export let onClose: () => void;
  export let onChange: (settings: DesktopSettings) => void;
  export let onDefaultListViewChange: (mode: DefaultListViewMode) => void | Promise<void>;

  let busy = false;
  let mounted = true;
  const statusLabels: Record<CapabilityStatus, TranslationKey> = {
    unknown: "settings.capabilityUnknown", enabled: "settings.capabilityEnabled",
    disabled: "settings.capabilityDisabled", inactive: "settings.capabilityInactive",
    unsupported: "settings.capabilityUnsupported",
  };
  let error = settings.shortcutError ?? settings.noteShortcutError ?? settings.autostartError ?? "";
  let focusDurationMinutes = 25;
  let breakDurationMinutes = 5;
  const languageOptions: Array<{ mode: LanguageMode; label: TranslationKey }> = [
    { mode: "system", label: "settings.languageSystem" },
    { mode: "zh-CN", label: "settings.languageSimplifiedChinese" },
    { mode: "en-US", label: "settings.languageEnglish" },
  ];

  onMount(() => {
    focusDurationMinutes = getFocusDurationMinutes();
    breakDurationMinutes = getBreakDurationMinutes();
    void refreshCapabilities();
    return () => { mounted = false; };
  });

  async function readCapabilities() {
    const actual = await refreshDesktopSettings();
    if (mounted) onChange(actual);
  }

  async function refreshCapabilities() {
    if (busy || !mounted) return;
    error = "";
    busy = true;
    try { await readCapabilities(); }
    finally { busy = false; }
  }

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
      await readCapabilities();
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
      await readCapabilities();
      busy = false;
    }
  }

  async function saveNoteShortcut(shortcut: string, enabled: boolean) {
    if (busy) return;
    busy = true;
    error = "";
    try {
      await updateNoteShortcut(shortcut, enabled);
      onChange({ ...settings, noteShortcut: shortcut, noteShortcutEnabled: enabled, noteShortcutError: null });
    } catch (reason) {
      error = reason instanceof Error ? reason.message : String(reason);
      onChange({ ...settings, noteShortcutError: error });
    } finally {
      await readCapabilities();
      busy = false;
    }
  }

  async function setFocusDuration(minutes: number) {
    focusDurationMinutes = await saveFocusDurationMinutes(minutes);
  }

  async function setBreakDuration(minutes: number) {
    breakDurationMinutes = await saveBreakDurationMinutes(minutes);
  }

  function selectLanguage(mode: LanguageMode) {
    setLanguageMode(mode);
  }

  async function selectDefaultView(element: HTMLSelectElement) {
    await onDefaultListViewChange(element.value as DefaultListViewMode);
    await tick();
    // A rejected save leaves the prop unchanged; restore the native select as well.
    element.value = defaultListViewMode;
  }
</script>

<style>
  .capability-retry {
    margin: 6px 0 12px;
    padding: 8px 14px;
    border: 1px solid var(--action-border);
    border-radius: 999px;
    background: var(--action-bg);
    color: var(--action-text);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .capability-retry:disabled { opacity: 0.55; cursor: default; }
  .capability-error { overflow-wrap: anywhere; }
  .setting-row div > span[role="status"] { font-size: 12px; color: #655943; }
  :global(html[data-theme="dark"]) .setting-row div > span[role="status"] {
    color: #dccfb5;
  }
</style>

<svelte:window
  onfocus={() => { void refreshCapabilities(); }}
  onkeydown={(event) => {
    if (event.key === "Escape" && !busy) onClose();
  }}
/>

<div class="settings-backdrop">
  <button
    class="settings-dismiss"
    type="button"
    aria-label={$translator("common.close")}
    onclick={onClose}
  ></button>
  <section class="settings-card" aria-labelledby="settings-title">
    <header>
      <div>
        <h2 id="settings-title">{$translator("settings.title")}</h2>
        <p>{$translator("settings.subtitle")}</p>
      </div>
      <button type="button" aria-label={$translator("common.close")} onclick={onClose}>×</button>
    </header>

    <PreferenceStatus />
    <WindowSettings />
    <section class="language-settings-section" aria-labelledby="language-settings-title">
      <div class="language-settings-heading">
        <strong id="language-settings-title">{$translator("settings.language")}</strong>
        <span>{$translator("settings.languageHelp")}</span>
      </div>
      <div
        class="language-options"
        role="group"
        aria-label={$translator("settings.language")}
      >
        {#each languageOptions as option}
          <button
            type="button"
            class:active={$languageState.mode === option.mode}
            aria-pressed={$languageState.mode === option.mode}
            onclick={() => selectLanguage(option.mode)}
          >
            {$translator(option.label)}
          </button>
        {/each}
      </div>
    </section>

    <div class="setting-row">
      <div>
        <strong>{$translator("settings.shortcutTitle")}</strong>
        <span>{$translator("settings.shortcutHelp")}</span>
        <span role="status">{$translator(statusLabels[settings.shortcutStatus ?? "unknown"])}</span>
        {#if settings.shortcutError}<span class="capability-error">{settings.shortcutError}</span>{/if}
      </div>
      <label class="switch">
        <input
          type="checkbox"
          aria-label={$translator("settings.shortcutTitle")}
          indeterminate={!settings.shortcutPreferenceKnown}
          checked={settings.shortcutEnabled}
          disabled={busy || !settings.shortcutPreferenceKnown || settings.shortcutStatus === "unsupported"}
          onchange={(event) =>
            void setShortcutEnabled(event.currentTarget.checked)}
        />
        <span></span>
      </label>
    </div>

    <label class="shortcut-select">
      <span>{$translator("settings.shortcutCombination")}</span>
      <select
        value={settings.shortcut}
        disabled={busy || !settings.shortcutPreferenceKnown || !settings.shortcutEnabled || settings.shortcutStatus === "unsupported"}
        onchange={(event) => void setShortcut(event.currentTarget.value)}
      >
        {#each shortcutOptions as option}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>

    {#if settings.shortcutPreferenceKnown && settings.shortcutEnabled && settings.shortcutStatus === "inactive"}
      <button class="capability-retry" type="button" disabled={busy} onclick={() => saveShortcut(settings.shortcut, true)}>{$translator("settings.capabilityRetryShortcut")}</button>
    {/if}

    <div class="setting-row">
      <div>
        <strong>{$translator("capture.shortcut")}</strong>
        <span>{$translator("capture.pending")}</span>
        <span role="status">{$translator(statusLabels[settings.noteShortcutStatus ?? "unknown"])}</span>
        {#if settings.noteShortcutError}<span class="capability-error">{settings.noteShortcutError}</span>{/if}
      </div>
      <label class="switch">
        <input type="checkbox" aria-label={$translator("capture.shortcut")} indeterminate={!settings.noteShortcutPreferenceKnown} checked={settings.noteShortcutEnabled} disabled={busy || !settings.noteShortcutPreferenceKnown || settings.noteShortcutStatus === "unsupported"}
          onchange={(event) => void saveNoteShortcut(settings.noteShortcut, event.currentTarget.checked)} />
        <span></span>
      </label>
    </div>
    <label class="shortcut-select">
      <span>{$translator("settings.shortcutCombination")}</span>
      <select value={settings.noteShortcut} disabled={busy || !settings.noteShortcutPreferenceKnown || !settings.noteShortcutEnabled || settings.noteShortcutStatus === "unsupported"}
        onchange={(event) => void saveNoteShortcut(event.currentTarget.value, settings.noteShortcutEnabled)}>
        {#each noteShortcutOptions as option}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </label>
    {#if settings.noteShortcutPreferenceKnown && settings.noteShortcutEnabled && settings.noteShortcutStatus === "inactive"}
      <button class="capability-retry" type="button" disabled={busy} onclick={() => saveNoteShortcut(settings.noteShortcut, true)}>{$translator("settings.capabilityRetryShortcut")}</button>
    {/if}
    <div class="setting-row">
      <div>
        <strong>{$translator("settings.autostartTitle")}</strong>
        <span>{$translator("settings.autostartHelp")}</span>
        <span role="status">{$translator(statusLabels[settings.autostartStatus ?? "unknown"])}</span>
        {#if settings.autostartError}<span class="capability-error">{settings.autostartError}</span>{/if}
      </div>
      <label class="switch">
        <input
          type="checkbox"
          checked={settings.autostartEnabled}
          aria-label={$translator("settings.autostartTitle")}
          indeterminate={settings.autostartStatus === "unknown"}
          disabled={busy || settings.autostartStatus === "unknown" || settings.autostartStatus === "unsupported"}
          onchange={(event) => void setAutostart(event.currentTarget.checked)}
        />
        <span></span>
      </label>
    </div>

    <button class="capability-retry" type="button" disabled={busy} onclick={refreshCapabilities}>
      {$translator(busy ? "settings.capabilityChecking" : "settings.capabilityRefresh")}
    </button>

    <label class="preference-select">
      <span>{$translator("settings.defaultView")}</span>
      <select
        value={defaultListViewMode}
        onchange={(event) => { void selectDefaultView(event.currentTarget); }}
      >
        <option value="remember">{$translator("settings.rememberLastView")}</option>
        <option value="all">{$translator("nav.all")}</option>
        <option value="today">{$translator("nav.today")}</option>
        <option value="quadrants">{$translator("nav.matrix")}</option>
        <option value="calendar">{$translator("nav.calendar")}</option>
      </select>
    </label>

    <section class="focus-settings-section" aria-labelledby="focus-settings-title">
      <div class="setting-row focus-settings-heading">
        <div>
          <strong id="focus-settings-title">{$translator("settings.focusDuration")}</strong>
          <span>{$translator("settings.focusDurationHelp")}</span>
        </div>
      </div>

      <div class="duration-setting">
        <span>{$translator("settings.focus")}</span>
        <div class="duration-options" role="group" aria-label={$translator("settings.focusDuration")}>
          {#each FOCUS_DURATION_OPTIONS as minutes}
            <button
              type="button"
              class:active={focusDurationMinutes === minutes}
              onclick={() => setFocusDuration(minutes)}
            >
              {$translator("settings.minutes", { count: minutes })}
            </button>
          {/each}
        </div>
      </div>

      <div class="duration-setting">
        <span>{$translator("settings.break")}</span>
        <div class="duration-options" role="group" aria-label={$translator("settings.break")}>
          {#each BREAK_DURATION_OPTIONS as minutes}
            <button
              type="button"
              class:active={breakDurationMinutes === minutes}
              onclick={() => setBreakDuration(minutes)}
            >
              {$translator("settings.minutes", { count: minutes })}
            </button>
          {/each}
        </div>
      </div>
    </section>

    <SyncSettings />

    {#if error}<p class="settings-error" role="alert">{error}</p>{/if}
  </section>
</div>
