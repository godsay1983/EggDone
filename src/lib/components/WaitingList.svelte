<script lang="ts">
  import { onMount } from 'svelte';
  import { translator } from '$lib/i18n';
  import { taskWorkflow, waitingRows, type WaitingFilter, type WaitingSort } from '$lib/stores/taskWorkflowStore';
  import type { Todo } from '$lib/types';
  import { reviewDateDisplay } from '$lib/utils/waitingReviewDate';
  export let items: Todo[];
  export let onClose: () => void;
  let dialog: HTMLDialogElement;
  let filter: WaitingFilter = 'all', sort: WaitingSort = 'date';
  $: rows = waitingRows($taskWorkflow, items, filter, sort);
  onMount(() => { dialog.showModal(); void taskWorkflow.refresh(); return () => dialog.close(); });
</script>

<dialog class="waiting-list" bind:this={dialog} aria-label={$translator('waiting.title')}
  onkeydown={e => e.stopPropagation()} oncancel={e => { e.preventDefault(); onClose(); }}>
  <header>
    <h2>{$translator('waiting.title')}</h2>
    {#if $taskWorkflow.snapshot}<span class="count">{rows.length}</span>{/if}
    <button class="close" aria-label={$translator('common.close')} title={$translator('common.close')} onclick={onClose}>×</button>
  </header>
  <div class="toolbar">
    <div class="filters" role="group" aria-label={$translator('waiting.filter')}>
      {#each ['all', 'due', 'undated'] as value}<button class:active={filter === value} aria-pressed={filter === value} onclick={() => filter = value as WaitingFilter}>{$translator(`waiting.filter.${value as WaitingFilter}`)}</button>{/each}
    </div>
    <label class="sort">{$translator('waiting.sort')}<select bind:value={sort}><option value="date">{$translator('waiting.sort.date')}</option><option value="updated">{$translator('waiting.sort.updated')}</option></select></label>
  </div>
  <div class="list-content" aria-busy={$taskWorkflow.loading}>
    {#if $taskWorkflow.error}<p role="alert">{$translator('waiting.error.load')} <button class="action-button" onclick={() => void taskWorkflow.refresh()}>{$translator('common.retry')}</button></p>
    {:else if $taskWorkflow.loading && !$taskWorkflow.snapshot}<p role="status">{$translator('common.loading')}</p>
    {:else if rows.length === 0}
      <div class="empty" role="status">
        <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8" /><path d="M12 7v5l3 2" /></svg>
        <p>{$translator('waiting.empty')}</p>
      </div>
    {/if}
    {#each rows as row (row.todo.uuid)}
      <div class="waiting-row" data-waiting-uuid={row.todo.uuid}>
        <slot todo={row.todo} />
        {#if row.entry.reason || row.entry.review_date}
          <div class="waiting-detail">
            {#if row.entry.reason}<p>{row.entry.reason}</p>{/if}
            {#if row.entry.review_date}<span class:review-due={row.entry.review_due}>{$translator('waiting.reviewOn', { date: reviewDateDisplay(row.entry.review_date) })}</span>{/if}
          </div>
        {/if}
      </div>
    {/each}
  </div>
</dialog>

<style>
  .waiting-list { --panel-bg: #fffdf8; --subtle-bg: #f3eee4; --muted-text: #82745e; --due-text: #945241; width: min(540px, calc(100vw - 24px)); height: fit-content; max-height: min(620px, calc(100dvh - 32px)); box-sizing: border-box; padding: 16px; border-radius: 8px; border: 1px solid var(--action-border); background: var(--panel-bg); color: var(--action-text); font-size: 13px; overflow: hidden; }
  :global(html[data-theme="dark"]) .waiting-list { --panel-bg: #29251e; --subtle-bg: #343027; --muted-text: #b1a590; --due-text: #f1bd9d; color-scheme: dark; }
  .waiting-list[open] { display: flex; flex-direction: column; gap: 12px; }
  .waiting-list::backdrop { background: rgb(0 0 0 / 38%); }
  header { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  h2 { font-size: 16px; margin: 0; }
  .count { padding: 2px 7px; border-radius: 6px; background: var(--subtle-bg); color: var(--muted-text); font-size: 11px; font-variant-numeric: tabular-nums; }
  .close { margin-left: auto; padding: 0; border: 0; border-radius: 6px; background: transparent; color: var(--muted-text); font-size: 22px; height: 28px; width: 28px; cursor: pointer; }
  .close:hover { background: var(--subtle-bg); color: var(--action-text); }
  .toolbar { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; padding-bottom: 12px; border-bottom: 1px solid var(--action-border); flex-shrink: 0; }
  .filters { display: flex; flex-wrap: wrap; gap: 2px; padding: 3px; border-radius: 6px; background: var(--subtle-bg); max-width: 100%; }
  .filters button { font: inherit; font-size: 12px; min-height: 28px; padding: 4px 9px; border: 0; border-radius: 4px; background: transparent; color: var(--muted-text); cursor: pointer; }
  .filters button:hover { color: var(--action-text); }
  .filters button.active { background: var(--action-primary-bg); color: var(--action-primary-text); border-color: var(--action-primary-bg); }
  .sort { display: flex; align-items: center; justify-content: flex-end; gap: 6px; margin-left: auto; font-size: 11px; color: var(--muted-text); }
  select { font: inherit; font-size: 12px; color: var(--action-text); max-width: 100%; padding: 5px 6px; background: var(--subtle-bg); border: 1px solid transparent; border-radius: 6px; color-scheme: inherit; }
  option { background: var(--panel-bg, #fffdf5); color: var(--action-text); }
  .list-content { flex: 0 1 auto; min-height: 0; overflow-y: auto; padding: 2px; scrollbar-width: thin; scrollbar-color: var(--action-border) transparent; }
  .waiting-row { padding-bottom: 12px; margin-bottom: 12px; border-bottom: 1px solid var(--action-border); }
  .waiting-row:last-child { padding-bottom: 0; margin-bottom: 0; border-bottom: 0; }
  .waiting-detail { margin: 4px 12px 2px; padding-left: 10px; border-left: 2px solid var(--action-border); font-size: 12px; color: var(--muted-text); overflow-wrap: anywhere; }
  .waiting-detail p { margin: 0 0 4px; white-space: pre-wrap; }
  .waiting-detail span { font-size: 11px; }
  .waiting-detail span.review-due { color: var(--due-text); }
  .empty { padding: 24px 12px 28px; text-align: center; color: var(--muted-text); }
  .empty svg { width: 28px; height: 28px; fill: none; stroke: currentColor; stroke-width: 1.4; opacity: .65; }
  .empty p { margin: 10px 0 0; font-size: 12px; line-height: 1.6; }
  button:focus-visible, select:focus-visible { outline: 2px solid var(--action-focus); outline-offset: 1px; }
</style>
