<script lang="ts">
  import { browser } from "$app/environment";
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";
  import FocusWindow from "$lib/components/FocusWindow.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import { initializeLanguage, languageState } from "$lib/i18n";
  import {
    applyFontScale,
    FONT_SCALE_CHANGED_EVENT,
    getFontScale,
    normalizeFontScale,
  } from "$lib/utils/fontScale";
  import "../app.css";

  if (browser) {
    initializeLanguage();
    // Apply the persisted font scale before the first paint to avoid a flash.
    applyFontScale(getFontScale());
  }

  let unlistenFontScale: UnlistenFn | null = null;

  onMount(async () => {
    if (isTauri()) {
      unlistenFontScale = await listen<string>(
        FONT_SCALE_CHANGED_EVENT,
        (event) => {
          applyFontScale(normalizeFontScale(event.payload));
        },
      );
    }
  });

  onDestroy(() => {
    unlistenFontScale?.();
  });

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
