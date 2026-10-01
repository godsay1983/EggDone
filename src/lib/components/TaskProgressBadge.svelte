<script lang="ts">
  import { translator } from '$lib/i18n';
  import { taskProgressCounts } from '$lib/stores/taskProgressCounts';
  export let uuid: string;
  export let always = false;
  export let menuItem = false;
  export let disabled = false;
  export let onOpen: () => void;
  $: count = $taskProgressCounts[uuid] ?? 0;
</script>
{#if always || count > 0}
  <button type="button" class:action-button={always} class:progress-badge={!always} role={menuItem ? 'menuitem' : undefined}
    {disabled} title={$translator('taskProgress.title')} onclick={onOpen}>
    {$translator(always ? 'taskProgress.title' : 'taskProgress.count', { count })}
  </button>
{/if}
<style>
  .progress-badge { display: inline-flex; align-items: center; max-width: 100%; padding: 2px 7px; border: 0; border-radius: 6px; background: var(--action-bg); color: var(--action-text); font: inherit; font-size: 11px; line-height: 1.4; white-space: normal; overflow-wrap: anywhere; cursor: pointer; }
  button:focus-visible { outline: 2px solid var(--action-focus); outline-offset: 2px; }
</style>
