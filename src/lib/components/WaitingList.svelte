<script lang="ts">
  import { onMount } from 'svelte';
  import { translator } from '$lib/i18n';
  import { taskWorkflow, waitingRows, type WaitingFilter, type WaitingSort } from '$lib/stores/taskWorkflowStore';
  import type { Todo } from '$lib/types';
  export let items: Todo[];
  export let onClose: () => void;
  let dialog: HTMLDialogElement;
  let filter: WaitingFilter = 'all', sort: WaitingSort = 'date';
  $: rows = waitingRows($taskWorkflow, items, filter, sort);
  onMount(() => { dialog.showModal(); void taskWorkflow.refresh(); return () => dialog.close(); });
</script>

<dialog class="waiting-list" bind:this={dialog} aria-label={$translator('waiting.title')}
  onkeydown={e => e.stopPropagation()} oncancel={e => { e.preventDefault(); onClose(); }}>
  <header><h2>{$translator('waiting.title')}</h2><button class="close" aria-label={$translator('common.close')} title={$translator('common.close')} onclick={onClose}>×</button></header>
  <div class="toolbar">
    <div class="filters" role="group" aria-label={$translator('waiting.filter')}>
      {#each ['all', 'due', 'undated'] as value}<button class:active={filter === value} aria-pressed={filter === value} onclick={() => filter = value as WaitingFilter}>{$translator(`waiting.filter.${value as WaitingFilter}`)}</button>{/each}
    </div>
    <label class="sort">{$translator('waiting.sort')}<select bind:value={sort}><option value="date">{$translator('waiting.sort.date')}</option><option value="updated">{$translator('waiting.sort.updated')}</option></select></label>
  </div>
  <div class="list-content" aria-busy={$taskWorkflow.loading}>
    {#if $taskWorkflow.error}<p role="alert">{$translator('waiting.error.load')} <button class="action-button" onclick={() => void taskWorkflow.refresh()}>{$translator('common.retry')}</button></p>
    {:else if $taskWorkflow.loading && !$taskWorkflow.snapshot}<p role="status">{$translator('common.loading')}</p>
    {:else if rows.length === 0}<p class="empty">{$translator('waiting.empty')}</p>{/if}
    {#each rows as row (row.todo.uuid)}
      <div class="waiting-row" data-waiting-uuid={row.todo.uuid}>
        <slot todo={row.todo} />
        {#if row.entry.reason || row.entry.review_date}
          <div class="waiting-detail">
            {#if row.entry.reason}<p>{row.entry.reason}</p>{/if}
            {#if row.entry.review_date}<span>{$translator('waiting.reviewOn', { date: row.entry.review_date })}</span>{/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
</dialog>

<style>
  .waiting-list { --panel-bg: #fffdf8; width: min(520px, calc(100vw - 24px)); height: min(620px, calc(100dvh - 32px)); box-sizing: border-box; padding: 14px; border-radius: 8px; border: 1px solid var(--action-border); background: var(--panel-bg); color: var(--action-text); font-size: 13px; }
  :global(html[data-theme="dark"]) .waiting-list { --panel-bg: #29251e; color-scheme: dark; }
  .waiting-list[open] { display: flex; flex-direction: column; gap: 12px; }
  .waiting-list::backdrop { background: rgb(0 0 0 / 38%); }
  header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
  h2 { font-size: 16px; margin: 0; }
  .close { border: 0; background: transparent; color: inherit; font-size: 22px; height: 32px; width: 32px; cursor: pointer; }
  .toolbar { display: flex; flex-direction: column; gap: 8px; }
  .filters { display: flex; gap: 4px; }
  .filters button { flex: 1; font: inherit; min-height: 32px; padding: 4px 6px; border: 1px solid var(--action-border); border-radius: 6px; background: var(--action-bg); color: inherit; cursor: pointer; }
  .filters button.active { background: var(--action-primary-bg); color: var(--action-primary-text); border-color: var(--action-primary-bg); }
  .sort { display: flex; align-items: center; justify-content: flex-end; gap: 8px; font-size: 12px; }
  select { font: inherit; color: inherit; padding: 5px 8px; background: var(--action-bg); border: 1px solid var(--action-border); border-radius: 6px; color-scheme: inherit; }
  option { background: var(--panel-bg, #fffdf5); color: var(--action-text); }
  .list-content { flex: 1; min-height: 0; overflow-y: auto; padding: 1px 2px 8px; }
  .waiting-row { margin-bottom: 10px; }
  .waiting-detail { margin: 5px 12px 0; font-size: 12px; opacity: .8; overflow-wrap: anywhere; }
  .waiting-detail p { margin: 0 0 4px; white-space: pre-wrap; }
  .waiting-detail span { font-size: 11px; }
  .empty { padding: 24px 0; text-align: center; }
  button:focus-visible, select:focus-visible { outline: 2px solid var(--action-focus); outline-offset: 1px; }
</style>
