<script lang="ts">
  import { translator } from '$lib/i18n';
  import { dailyPlans, dailyPlanLocked, dailyPlanRows } from '$lib/stores/dailyPlanStore';
  import type { Todo } from '$lib/types';
  export let items: Todo[];
  export let query = '';
  export let groupUuid: string | null | undefined = undefined;
  export let onOpen: (todo: Todo) => void;
  export let onToggle: (todo: Todo) => Promise<void>;
  let toggling: string | null = null;
  let toggleFailed = false;
  $: rows = dailyPlanRows($dailyPlans.snapshot, items);
  $: locked = dailyPlanLocked($dailyPlans) || toggling !== null;
  $: filtered = {
    planned: rows.planned.filter(todo => matches(todo, query, groupUuid)),
    completed: rows.completed.filter(todo => matches(todo, query, groupUuid)),
    previous: rows.previous.filter(todo => matches(todo, query, groupUuid)),
  };
  function matches(todo: Todo, query: string, groupUuid: string | null | undefined) {
    return (groupUuid === undefined || todo.group_uuid === groupUuid) &&
      `${todo.title}\n${todo.note ?? ''}`.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase());
  }
  async function complete(todo: Todo) {
    if (locked) return;
    toggling = todo.uuid; toggleFailed = false;
    try { await onToggle(todo); await dailyPlans.refresh(); }
    catch { toggleFailed = true; }
    finally { toggling = null; }
  }
</script>

<section class="daily-plan-list" aria-label={$translator('dailyPlan.title')} aria-busy={$dailyPlans.loading || $dailyPlans.writing}>
  {#if toggleFailed}<p role="alert">{$translator('dailyPlan.completeFailed')}</p>{/if}
  {#if $dailyPlans.snapshot}
    {#if filtered.planned.length === 0}
      <p class="empty">{$translator(query.trim() || groupUuid !== undefined ? 'search.noMatch' : 'dailyPlan.empty')}</p>
    {/if}
    {#each filtered.planned as todo (todo.uuid)}
      {@const index = rows.planned.findIndex(item => item.uuid === todo.uuid)}
      <div class="todo-row" data-plan-uuid={todo.uuid}>
        <slot {todo} {locked} onToggle={complete}
          canPlanMoveUp={index > 0} canPlanMoveDown={index < rows.planned.length - 1} />
      </div>
    {/each}
    {#if filtered.completed.length}
      {#key $dailyPlans.date}
        <details class="completed-plans">
          <summary>{$translator('dailyPlan.completed', { count: filtered.completed.length })}</summary>
          {#each filtered.completed as todo (todo.uuid)}<p class="completed-title">{todo.title}</p>{/each}
        </details>
      {/key}
    {/if}
    {#if filtered.previous.length}
      <section class="previous-plans" aria-label={$translator('dailyPlan.previous')}>
        <h3>{$translator('dailyPlan.previous')}</h3>
        {#each filtered.previous as todo (todo.uuid)}
          <article class="previous-row" data-previous-uuid={todo.uuid}>
            <button class="task-title" onclick={() => onOpen(todo)}>{todo.title}</button>
            <button class="action-button" disabled={locked} onclick={() => void dailyPlans.act(todo.uuid, 'add')}>{$translator('dailyPlan.continue')}</button>
          </article>
        {/each}
      </section>
    {/if}
  {/if}
</section>

<style>
  .daily-plan-list { min-height: 0; overflow-y: auto; overflow-x: hidden; padding: 1px 2px 8px; flex: 1; color: var(--action-text); }
  .task-title { text-align: left; border: 0; background: transparent; font: inherit; font-size: 13px; min-width: 0; overflow-wrap: anywhere; padding: 6px 2px; cursor: pointer; }
  .completed-plans { padding: 8px 0; font-size: 12px; }
  .completed-title { text-decoration: line-through; overflow-wrap: anywhere; margin: 8px 4px; }
  .previous-plans { border-top: 1px solid var(--action-border); margin-top: 8px; padding-top: 8px; }
  h3 { font-size: 13px; margin: 0 0 6px; }
  .previous-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px; align-items: start; margin-bottom: 6px; }
  .previous-row .action-button { max-width: 100px; }
  summary { cursor: pointer; overflow-wrap: anywhere; }
  summary:focus-visible, .task-title:focus-visible { outline: 2px solid var(--action-focus); outline-offset: 1px; }
  .empty { font-size: 13px; padding: 16px 6px; text-align: center; overflow-wrap: anywhere; }
</style>
