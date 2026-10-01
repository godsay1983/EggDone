<script lang="ts">
  import { onMount } from 'svelte';
  import { taskProgressCounts, watchTaskProgress } from '$lib/stores/taskProgressCounts';
  import TaskProgressBadge from './TaskProgressBadge.svelte';
  import TaskProgressDialog from './TaskProgressDialog.svelte';
  export let uuid: string;
  export let title: string;
  export let readOnly = false;
  export let always = false;
  export let disabled = false;
  let open = false;
  let registration: ReturnType<typeof taskProgressCounts.register> | null = null;
  $: registration?.update([uuid]);
  onMount(() => {
    registration = taskProgressCounts.register([uuid]);
    const stop = watchTaskProgress();
    return () => { registration?.dispose(); stop(); };
  });
</script>
<TaskProgressBadge {uuid} {always} {disabled} onOpen={() => open = true} />
{#if open}<TaskProgressDialog {uuid} {title} {readOnly} onClose={() => open = false} />{/if}
