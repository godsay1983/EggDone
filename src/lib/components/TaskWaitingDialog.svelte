<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { translator } from '$lib/i18n';
  import { taskWorkflow, waitingEntry, createWorkflowWriter, validReviewDate, workflowError, type WorkflowError } from '$lib/stores/taskWorkflowStore';
  import { dailyPlans, isPlannedToday } from '$lib/stores/dailyPlanStore';
  import { scheduleAutoSync } from '$lib/sync/autoSync';
  import { localDateString } from '$lib/utils/todoDates';
  import { reviewDateValue, reviewDateDisplay } from '$lib/utils/waitingReviewDate';
  import type { Todo } from '$lib/types';
  export let todo: Todo;
  export let resume = false;
  export let onClose: () => void;
  const writer = createWorkflowWriter();
  let dialog: HTMLDialogElement;
  let reason = '', reviewDateText = '', remove = false, busy = false, loading = true;
  $: reviewDate = reviewDateValue(reviewDateText);
  $: pickerDate = validReviewDate(reviewDate) ? reviewDate : '';
  let loaded = false, expected = '', expectedPlan: string | null = null, date = '', planned = false;
  let retry = false, existing = false, error: WorkflowError | null = null;
  async function load(replaceDraft: boolean) {
    loading = true; loaded = false; error = null;
    const snapshot = await taskWorkflow.refresh();
    await dailyPlans.refresh();
    if (!snapshot) { error = 'load'; loading = false; return; }
    expected = snapshot.revision; date = snapshot.date;
    expectedPlan = $dailyPlans.snapshot?.date === date ? $dailyPlans.snapshot.revision : null;
    planned = isPlannedToday($dailyPlans, todo, date);
    if (!planned) remove = false;
    existing = waitingEntry($taskWorkflow, todo, date) !== undefined;
    if (replaceDraft) {
      const entry = waitingEntry($taskWorkflow, todo, date);
      reason = entry?.reason ?? ''; reviewDateText = reviewDateDisplay(entry?.review_date ?? ''); remove = false;
    }
    loaded = true; loading = false;
    await tick(); dialog.querySelector<HTMLTextAreaElement>('textarea')?.focus();
  }
  onMount(() => { dialog.showModal(); void load(true); return () => dialog.close(); });
  async function save() {
    if (busy || loading || !loaded) return;
    if (date !== localDateString()) {
      writer.discard(); retry = false; remove = false;
      await load(false); error = 'rollover'; return;
    }
    if (!resume && (reason.length > 200 || !validReviewDate(reviewDate) || (remove && !expectedPlan))) {
      error = 'invalid'; return;
    }
    busy = true; error = null;
    try {
      await writer.save(retry ? undefined : {
        task_uuid: todo.uuid, state: resume ? 'ready' : 'waiting', reason: resume ? '' : reason,
        review_date: resume ? null : reviewDate || null, date,
        remove_from_plan: !resume && remove, expected, expected_plan: !resume && remove ? expectedPlan : null,
      });
    } catch (e) {
      error = workflowError(e); retry = writer.pending !== null; busy = false;
      if (error === 'conflict' || error === 'unavailable') {
        const savedError = error; await load(false); error = savedError;
      }
      return;
    }
    // A refresh/notification failure must not turn a committed write into a second operation.
    retry = false;
    try { scheduleAutoSync(); } catch { /* Sync status owns notification errors. */ }
    await taskWorkflow.refresh(); await dailyPlans.refresh();
    busy = false; onClose();
  }
  async function refreshAfterFailure() {
    writer.discard(); retry = false; await load(false);
  }
</script>

<dialog class="waiting-editor" bind:this={dialog} aria-label={$translator(resume ? 'waiting.resume' : existing ? 'waiting.edit' : 'waiting.set')}
  onkeydown={e => e.stopPropagation()} oncancel={e => { e.preventDefault(); if (!busy) onClose(); }}>
  <header><h2>{$translator(resume ? 'waiting.resume' : existing ? 'waiting.edit' : 'waiting.set')}</h2></header>
  <p class="task-name">{todo.title}</p>
  <div class="editor-body" aria-busy={loading || busy}>
    {#if loading}<p role="status">{$translator('common.loading')}</p>{/if}
    {#if resume}
      <p class="resume-hint">{$translator('waiting.resumeHint')}</p>
    {:else}
      <label for="waiting-reason">{$translator('waiting.reason')}</label>
      <textarea id="waiting-reason" bind:value={reason} maxlength={200} rows={3} disabled={busy || loading || retry}></textarea>
      <span class="character-count">{reason.length} / 200</span>
      <label for="waiting-date">{$translator('waiting.reviewDate')}</label>
      <div class="date-field">
        <div class="date-control">
          <input id="waiting-date" type="text" placeholder="yyyy/mm/dd" inputmode="numeric" autocomplete="off"
            bind:value={reviewDateText} onblur={() => { if (validReviewDate(reviewDate)) reviewDateText = reviewDateDisplay(reviewDate); }}
            disabled={busy || loading || retry} />
          <span class="calendar-picker" class:disabled={busy || loading || retry}>
            <svg viewBox="0 0 20 20" aria-hidden="true"><rect x="3" y="4" width="14" height="13" rx="2" /><path d="M6 2v4m8-4v4M3 8h14M6 11h2m2 0h2m2 0h1M6 14h2m2 0h2" /></svg>
            <input class="calendar-input" type="date" min="0001-01-01" max="9999-12-31" value={pickerDate}
              aria-label={$translator('waiting.reviewDate')} title={$translator('waiting.reviewDate')}
              oninput={event => reviewDateText = reviewDateDisplay(event.currentTarget.value)} disabled={busy || loading || retry} />
          </span>
        </div>
        {#if reviewDateText}<button class="text-action" disabled={busy || loading || retry} onclick={() => reviewDateText = ''}>{$translator('waiting.clearDate')}</button>{/if}
      </div>
      {#if planned}
        <label class="check-label"><input type="checkbox" bind:checked={remove} disabled={busy || loading || retry || expectedPlan === null} />
          <span>{$translator('waiting.removePlan')}</span></label>
      {/if}
    {/if}
    {#if error}
      <p class="error" role="alert">{$translator(`waiting.error.${error}`)}</p>
      {#if error === 'load' || retry}<button class="text-action" disabled={busy || loading} onclick={refreshAfterFailure}>{$translator('waiting.refresh')}</button>{/if}
    {/if}
  </div>
  <footer>
    <button class="action-button" disabled={busy} onclick={onClose}>{$translator('common.cancel')}</button>
    <button class="action-button" data-tone="primary" disabled={busy || loading || !loaded || todo.completed || todo.deleted_at !== null || todo.archived_at !== null} onclick={save}>
      {$translator(busy ? 'waiting.saving' : retry ? 'dailyPlan.retry' : resume ? 'waiting.resume' : 'common.save')}
    </button>
  </footer>
</dialog>

<style>
  .waiting-editor { --panel-bg: #fffdf8; width: min(400px, calc(100vw - 32px)); max-height: calc(100dvh - 32px); box-sizing: border-box; padding: 16px; border: 1px solid var(--action-border); border-radius: 8px; background: var(--panel-bg); color: var(--action-text); font-size: 13px; }
  :global(html[data-theme="dark"]) .waiting-editor { --panel-bg: #29251e; color-scheme: dark; }
  .waiting-editor::backdrop { background: rgb(0 0 0 / 38%); }
  header h2 { font-size: 16px; margin: 0; }
  .task-name { font-weight: 500; margin: 12px 0; overflow-wrap: anywhere; }
  .editor-body { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  label { font-size: 12px; }
  textarea, input[type=text] { width: 100%; box-sizing: border-box; min-width: 0; padding: 8px 10px; border: 1px solid var(--action-border); border-radius: 6px; background: var(--action-bg); color: inherit; font: inherit; }
  textarea { resize: vertical; min-height: 78px; max-height: 180px; }
  textarea:focus-visible, input:focus-visible, button:focus-visible { outline: 2px solid var(--action-focus); outline-offset: 2px; }
  .character-count { text-align: right; font-size: 11px; opacity: .65; }
  .date-field { display: flex; gap: 8px; align-items: center; }
  .date-control { flex: 1; position: relative; min-width: 0; }
  .date-control input[type=text] { padding-right: 38px; }
  .date-control input::placeholder { color: inherit; opacity: .65; }
  .calendar-picker { position: absolute; right: 3px; top: 2px; bottom: 2px; width: 30px; display: grid; place-items: center; border-radius: 4px; }
  .calendar-picker:focus-within { outline: 2px solid var(--action-focus); }
  .calendar-picker.disabled { opacity: .4; }
  .calendar-picker svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.5; }
  .calendar-input { position: absolute; inset: 0; width: 100%; height: 100%; min-width: 0; opacity: 0; cursor: pointer; }
  .calendar-input::-webkit-calendar-picker-indicator { position: absolute; inset: 0; width: 100%; height: 100%; margin: 0; padding: 0; cursor: pointer; }
  .check-label { display: flex; align-items: center; gap: 8px; margin-top: 4px; }
  .check-label input { width: 16px; height: 16px; margin: 0; flex: none; accent-color: var(--action-focus); }
  .text-action { align-self: flex-start; border: 0; background: transparent; color: inherit; font: inherit; text-decoration: underline; cursor: pointer; padding: 6px 0; }
  .resume-hint, .error { margin: 0; line-height: 1.5; overflow-wrap: anywhere; }
  .error { color: var(--danger-text, #c44c40); }
  footer { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; padding-top: 12px; border-top: 1px solid var(--action-border); }
  footer .action-button { font-size: 13px; min-height: 32px; padding: 6px 12px; }
</style>
