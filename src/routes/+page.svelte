<script lang="ts">
  import { browser } from "$app/environment";
  import FocusWindow from "$lib/components/FocusWindow.svelte";
  import TodoPanel from "$lib/components/TodoPanel.svelte";
  import "../app.css";

  const isFocusWindow =
    browser && new URLSearchParams(window.location.search).get("window") === "focus";
</script>

{#if isFocusWindow}
  <FocusWindow />
{:else}
  <TodoPanel />
{/if}
