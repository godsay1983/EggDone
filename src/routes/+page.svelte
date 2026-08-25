<script lang="ts">
  import { browser } from "$app/environment";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import FocusWindow from "$lib/components/FocusWindow.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import { initializeLanguage, languageState } from "$lib/i18n";
  import { applyFontScale, getFontScale } from "$lib/utils/fontScale";
  import "../app.css";

  if (browser) {
    initializeLanguage();
    // Apply the persisted font scale before the first paint to avoid a flash.
    applyFontScale(getFontScale());
  }

  let nativeLocale = "";
  $: if (browser && nativeLocale !== $languageState.resolvedLocale) {
    nativeLocale = $languageState.resolvedLocale;
    if (isTauri()) {
      void invoke("set_runtime_locale", { locale: nativeLocale }).catch(() => {});
    }
  }

  const isFocusWindow =
    browser && new URLSearchParams(window.location.search).get("window") === "focus";
</script>

{#if isFocusWindow}
  <FocusWindow />
{:else}
  <TodoPanel />
{/if}
