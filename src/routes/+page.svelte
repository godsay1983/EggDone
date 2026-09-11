<script lang="ts">
  import { browser } from "$app/environment";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import FocusWindow from "$lib/components/FocusWindow.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import { initializeWindowPreferences } from "$lib/stores/windowPreferences";
  import { onMount } from "svelte";
  import { initializeLanguage, languageState } from "$lib/i18n";
  import { initializePreferences } from '$lib/utils/preferenceStorage';
  import "../app.css";

  let preferencesLoaded = false;

  let nativeLocale = "";
  $: if (browser && nativeLocale !== $languageState.resolvedLocale) {
    nativeLocale = $languageState.resolvedLocale;
    if (isTauri()) {
      void invoke("set_runtime_locale", { locale: nativeLocale }).catch(() => {});
    }
  }

  const isFocusWindow =
    browser && new URLSearchParams(window.location.search).get("window") === "focus";
  onMount(() => {
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void initializePreferences().then(() => {
      if (disposed) return;
      initializeLanguage();
      preferencesLoaded = true;
    });
    const refreshPreferences = () => {
      if (preferencesLoaded && isTauri() && !document.hidden) void initializePreferences();
    };
    window.addEventListener('focus', refreshPreferences);
    document.addEventListener('visibilitychange', refreshPreferences);
    if (!isFocusWindow) {
      void initializeWindowPreferences().then(stop => { if (disposed) stop(); else cleanup = stop; });
    }
    return () => {
      disposed = true;
      cleanup?.();
      window.removeEventListener('focus', refreshPreferences);
      document.removeEventListener('visibilitychange', refreshPreferences);
    };
  });
</script>

{#if !preferencesLoaded}
  <div aria-busy="true"></div>
{:else if isFocusWindow}
  <FocusWindow />
{:else}
  <TodoPanel />
  <WindowControls />
{/if}
