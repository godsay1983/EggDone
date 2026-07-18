<script lang="ts">
  import {
    shortcutOptions,
    updateAutostart,
    updateShortcut,
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
  import { onMount } from "svelte";
  import SyncSettings from "./SyncSettings.svelte";

  export let settings: DesktopSettings;
  export let defaultListViewMode: DefaultListViewMode;
  export let onClose: () => void;
  export let onChange: (settings: DesktopSettings) => void;
  export let onDefaultListViewChange: (mode: DefaultListViewMode) => void;

  let busy = false;
  let error = settings.shortcutError ?? settings.autostartError ?? "";
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
  });

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

  function setFocusDuration(minutes: number) {
    focusDurationMinutes = saveFocusDurationMinutes(minutes);
  }

  function setBreakDuration(minutes: number) {
    breakDurationMinutes = saveBreakDurationMinutes(minutes);
  }

  function selectLanguage(mode: LanguageMode) {
    setLanguageMode(mode);
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
      <span>{$translator("settings.shortcutCombination")}</span>
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
        <strong>{$translator("settings.autostartTitle")}</strong>
        <span>{$translator("settings.autostartHelp")}</span>
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

    <label class="preference-select">
      <span>{$translator("settings.defaultView")}</span>
      <select
        value={defaultListViewMode}
        onchange={(event) =>
          onDefaultListViewChange(
            event.currentTarget.value as DefaultListViewMode,
          )}
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
