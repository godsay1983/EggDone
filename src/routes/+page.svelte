<script lang="ts">
  import { browser } from "$app/environment";
  import FocusWindow from "$lib/components/FocusWindow.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import { initializeLanguage } from "$lib/i18n";
  import "../app.css";

  if (browser) initializeLanguage();

  const isFocusWindow =
    browser && new URLSearchParams(window.location.search).get("window") === "focus";
</script>

{#if isFocusWindow}
  <FocusWindow />
{:else}
  <TodoPanel />
{/if}
