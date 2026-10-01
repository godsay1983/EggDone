<script lang="ts">
  import { onDestroy } from 'svelte';
  import { taskProgressPanel } from '$lib/stores/taskProgressPanel';
  import TaskProgressDialog from './TaskProgressDialog.svelte';
  // Keep the editor alive when a sync changes filtering, grouping or task lifecycle.
  onDestroy(() => taskProgressPanel.set(null));
</script>
{#if $taskProgressPanel}
  {#key $taskProgressPanel.uuid}
    <TaskProgressDialog uuid={$taskProgressPanel.uuid} title={$taskProgressPanel.title} onClose={() => taskProgressPanel.set(null)} />
  {/key}
{/if}
