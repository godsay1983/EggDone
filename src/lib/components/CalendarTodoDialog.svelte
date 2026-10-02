<script lang="ts">
  import { onMount } from 'svelte';
  import { translator } from '$lib/i18n';
  import type { CalendarTodoDraft } from '$lib/api/calendarTodoApi';
  import type { TodoGroup } from '$lib/types';
  import { createCalendarTodoSession } from '$lib/stores/calendarTodoStore';
  import { todos } from '$lib/stores/todoStore';
  import { calendarTodoIssue } from '$lib/utils/calendarTodoDraft';
  export let draft: CalendarTodoDraft;
  export let groups: TodoGroup[] = [];
  export let onClose: () => void;
  export let onView: (uuid: string) => void;
  const session = createCalendarTodoSession(draft, undefined, result => todos.calendarCommitted(result.todo));
  let dialog: HTMLDialogElement;
  $: issue = calendarTodoIssue($session.draft);
  $: locked = $session.busy || $session.uncertain || !!$session.result || $session.error === 'unavailable';
  const errorKeys = { group: 'calendarTodo.groupError', invalid: 'calendarTodo.invalid', unavailable: 'calendarTodo.unavailable', retry: 'calendarTodo.retry', uncertain: 'calendarTodo.uncertain' } as const;
  $: errorKey = $session.error ? errorKeys[$session.error] : null;
  function fitDialog() {
    const scale = dialog.getBoundingClientRect().width / dialog.offsetWidth || 1;
    const height = Math.max(80, (window.innerHeight - 24) / scale);
    dialog.style.maxHeight = `${height}px`;
    dialog.style.maxWidth = `${(window.innerWidth - 24) / scale}px`;
    const form = dialog.querySelector('form');
    if (form) form.style.maxHeight = `${height - 2}px`;
  }
  onMount(() => {
    dialog.showModal(); fitDialog(); window.addEventListener('resize', fitDialog);
    const observer = new MutationObserver(fitDialog);
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ['style'] });
    return () => { observer.disconnect(); window.removeEventListener('resize', fitDialog); dialog.close(); };
  });
</script>

<dialog bind:this={dialog} class="calendar-todo-dialog" aria-label={$translator('calendarTodo.title')}
  onkeydown={event => event.stopPropagation()} oncancel={event => { event.preventDefault(); if (!$session.busy) onClose(); }}>
  <form onsubmit={event => { event.preventDefault(); void session.save(); }}>
    <header><h2>{$translator('calendarTodo.title')}</h2>
      <button type="button" class="action-button close" disabled={$session.busy} aria-label={$translator('common.close')} onclick={onClose}>×</button></header>
    <div class="fields">
      {#if $session.result}
        <p role="status">{$translator('calendarTodo.saved')}</p>
        <strong class="saved-title">{$session.result.todo.title}</strong>
      {:else}
        <label>{$translator('calendarTodo.heading')}<input value={$session.draft.title} disabled={locked}
          oninput={event => session.edit({ title: event.currentTarget.value })} /></label>
        <label>{$translator('calendarTodo.note')}<textarea value={$session.draft.note} rows="6" disabled={locked}
          oninput={event => session.edit({ note: event.currentTarget.value })}></textarea></label>
        <small class:invalid={$session.draft.note.length > 1000}>{$session.draft.note.length} / 1000</small>
        <label>{$translator('calendarTodo.group')}<select value={$session.draft.group_uuid ?? ''} disabled={locked}
          onchange={event => session.edit({ group_uuid: event.currentTarget.value || null })}>
          <option value="">{$translator('calendarTodo.ungrouped')}</option>
          {#each groups.filter(group => group.deleted_at === null) as group (group.uuid)}<option value={group.uuid}>{group.name}</option>{/each}
        </select></label>
        <p class="privacy">{$translator('calendarTodo.privacy')}</p>
        {#if issue}<p class="invalid" role="alert">{$translator(issue === 'title' ? 'calendarTodo.titleIssue' : 'calendarTodo.noteIssue')}</p>{/if}
      {/if}
      {#if errorKey}<p class="invalid" role="alert">{$translator(errorKey)}</p>{/if}
    </div>
    <footer>
      <button type="button" class="action-button" disabled={$session.busy} onclick={onClose}>{$translator($session.result ? 'common.close' : 'common.cancel')}</button>
      {#if $session.result}
        <button type="button" class="action-button" data-tone="primary" onclick={() => { const uuid = $session.result!.todo.uuid; onClose(); onView(uuid); }}>{$translator('calendarTodo.view')}</button>
      {:else}
        <button type="submit" class="action-button" data-tone="primary" disabled={$session.busy || $session.error === 'unavailable' || (!$session.uncertain && !!issue)}>
          {$translator($session.uncertain ? 'calendarTodo.resolve' : 'calendarTodo.save')}</button>
      {/if}
    </footer>
  </form>
</dialog>

<style>
  .calendar-todo-dialog { --panel: #fffaf0; --input: #fff; --muted: #70664f; --error: #a03127;
    box-sizing: border-box; width: min(480px, calc(100% - 24px)); max-height: calc(100dvh - 24px);
    padding: 0; border: 1px solid var(--action-border, #d5c8ac); border-radius: 8px; background: var(--panel); color: var(--action-text, #463e31); }
  .calendar-todo-dialog::backdrop { background: #0006; }
  form { display: flex; flex-direction: column; max-height: calc(100dvh - 28px); }
  header, footer { display: flex; align-items: center; gap: 8px; padding: 12px 16px; flex: none; }
  header { justify-content: space-between; } footer { justify-content: flex-end; flex-wrap: wrap; border-top: 1px solid var(--action-border, #d5c8ac); }
  h2 { font-size: 16px; margin: 0; } .close { width: 32px; height: 32px; padding: 0; }
  .fields { padding: 0 16px 12px; overflow-y: auto; min-height: 0; }
  label { display: grid; gap: 6px; margin: 12px 0 6px; font-size: 13px; }
  input, textarea, select { box-sizing: border-box; width: 100%; min-width: 0; padding: 9px; border: 1px solid var(--action-border, #c7bda8); border-radius: 6px; background: var(--input); color: inherit; font: inherit; }
  textarea { min-height: 90px; resize: vertical; } small, .privacy { color: var(--muted); font-size: 12px; }
  .privacy, .invalid, .saved-title { overflow-wrap: anywhere; white-space: pre-wrap; } .invalid { color: var(--error); }
  :global(html[data-theme='dark']) .calendar-todo-dialog { --panel: #29251d; --input: #473c2d; --muted: #d2c4a9; --error: #ffc3b2; color: #f5ead0; }
</style>
