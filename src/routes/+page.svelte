<script lang="ts">
  import { browser } from "$app/environment";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import FocusWindow from "$lib/components/FocusWindow.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import WindowControls from "$lib/components/WindowControls.svelte";
  import { initializeWindowPreferences } from "$lib/stores/windowPreferences";
  import { onMount } from "svelte";
  import { initializeLanguage, languageState } from "$lib/i18n";
  import "../app.css";

  if (browser) initializeLanguage();

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
    if (isFocusWindow) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void initializeWindowPreferences().then(stop => { if (disposed) stop(); else cleanup = stop; });
    return () => { disposed = true; cleanup?.(); };
  });
</script>

{#if isFocusWindow}
  <FocusWindow />
{:else}
  <TodoPanel />
  <WindowControls />
{/if}
