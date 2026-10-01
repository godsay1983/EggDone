<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { languageState, translator } from '$lib/i18n';
  import { createTaskProgressSession } from '$lib/stores/taskProgressStore';
  import { taskProgressCommitted, taskProgressCounts, watchTaskProgress } from '$lib/stores/taskProgressCounts';
  import { normalizeProgressBody } from '$lib/utils/taskProgressBody';
  import type { ProgressView } from '$lib/types/taskProgress';
  export let uuid: string;
  export let title: string;
  export let readOnly = false;
  export let onClose: () => void;
  export let onChanged: () => void = () => {};
  const session = createTaskProgressSession(uuid, undefined, page => { taskProgressCommitted(page); onChanged(); });
  let dialog: HTMLDialogElement, confirmation: HTMLDialogElement, input: HTMLTextAreaElement;
  let confirming: 'close' | 'cancel' | 'delete' | 'edit' | null = null;
  let selected: ProgressView | null = null, menu: string | null = null;
  $: validated = normalizeProgressBody($session.draft);
  $: locked = readOnly || !$session.page || $session.page.read_only || $session.loadFailed;
  $: busy = $session.busy || $session.loading;
  $: dates = new Intl.DateTimeFormat($languageState.resolvedLocale, { dateStyle: 'medium' });
  $: times = new Intl.DateTimeFormat($languageState.resolvedLocale, { timeStyle: 'short' });
  function day(value: number) { return new Date(value).toDateString(); }
  function request(action: 'close' | 'cancel' | 'delete' | 'edit', entry: ProgressView | null = null) {
    if (busy) return;
    menu = null;
    if (action === 'delete' || $session.draft.length || $session.pending) { selected = entry; confirming = action; }
    else if (action === 'close') onClose();
    else if (action === 'cancel') session.cancelEdit();
    else if (entry) { session.edit(entry); void tick().then(() => input?.focus()); }
  }
  function confirm() {
    const action = confirming, entry = selected;
    confirming = null; selected = null;
    if (action === 'close') onClose();
    else if (action === 'delete' && entry) void session.remove(entry);
    else { session.cancelEdit(); if (action === 'edit' && entry) { session.edit(entry); void tick().then(() => input?.focus()); } }
  }
  function showConfirmation(node: HTMLDialogElement) { node.showModal(); return { destroy: () => node.close() }; }
  function floatMenu(node: HTMLDivElement) {
    const anchor = node.previousElementSibling as HTMLElement;
    function position() {
      const rect = anchor.getBoundingClientRect();
      const scale = node.getBoundingClientRect().width / node.offsetWidth || 1;
      node.style.maxHeight = `${Math.max(32, (window.innerHeight - 16) / scale)}px`;
      const size = node.getBoundingClientRect();
      node.style.left = `${Math.max(8, Math.min(rect.right - size.width, window.innerWidth - size.width - 8)) / scale}px`;
      node.style.top = `${Math.max(8, Math.min(rect.bottom + 4, window.innerHeight - size.height - 8)) / scale}px`;
    }
    node.showPopover(); position();
    const onToggle = (event: Event) => { if ((event as ToggleEvent).newState === 'closed') menu = null; };
    node.addEventListener('toggle', onToggle);
    window.addEventListener('resize', position); window.addEventListener('scroll', position, true);
    return { destroy() { node.removeEventListener('toggle', onToggle); window.removeEventListener('resize', position); window.removeEventListener('scroll', position, true); } };
  }
  onMount(() => {
    dialog.showModal(); void session.refresh();
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && dialog.open && !confirming && menu === null) {
        event.preventDefault(); event.stopImmediatePropagation(); request('close');
      }
    };
    window.addEventListener('keydown', escape, true);
    const stopWatching = watchTaskProgress();
    const stopChanges = taskProgressCounts.onChanged(() => { menu = null; void session.refresh(); });
    return () => { window.removeEventListener('keydown', escape, true); stopChanges(); stopWatching(); session.dispose(); dialog.close(); };
  });
</script>

<dialog class="progress-dialog" bind:this={dialog} aria-label={$translator('taskProgress.title')}
  onkeydown={event => event.stopPropagation()} oncancel={event => { event.preventDefault(); request('close'); }}>
  <header><h2>{$translator('taskProgress.title')}</h2>
    <button class="action-button" title={$translator('common.close')} aria-label={$translator('common.close')} disabled={busy} onclick={() => request('close')}>×</button></header>
  <p class="task-title">{$session.page?.title ?? title}</p>
  {#if $session.page?.overwritten}
    <div class="notice" role="status"><span>{$translator('taskProgress.overwritten')}</span>
      <button class="action-button" disabled={busy} onclick={() => session.dismissNotice()}>{$translator('taskProgress.dismiss')}</button></div>
  {/if}
  {#if locked && $session.page}<p class="status">{$translator('taskProgress.readOnly')}</p>{/if}
  <div class="entries" aria-busy={busy}>
    {#if $session.loading}<p role="status">{$translator('common.loading')}</p>{/if}
    {#if $session.page && !$session.page.entries.length}<p class="empty">{$translator('taskProgress.empty')}</p>{/if}
    {#each $session.page?.entries ?? [] as entry, index (entry.record.uuid)}
      {#if index === 0 || day(entry.record.created_at) !== day($session.page!.entries[index - 1].record.created_at)}
        <h3>{dates.format(entry.record.created_at)}</h3>
      {/if}
      <article>
        <div class="record-meta"><time datetime={new Date(entry.record.created_at).toISOString()}>{times.format(entry.record.created_at)}</time>
          {#if entry.record.clock > 1}<span title={dates.format(entry.record.updated_at) + ' ' + times.format(entry.record.updated_at)}>{$translator('taskProgress.edited')}</span>{/if}
          {#if !locked}
            <div class="record-actions"><button class="action-button" title={$translator('common.more')} aria-label={$translator('common.more')} aria-haspopup="menu" aria-expanded={menu === entry.record.uuid}
              disabled={busy || !!$session.pending} onclick={() => menu = menu === entry.record.uuid ? null : entry.record.uuid}>⋯</button>
              {#if menu === entry.record.uuid}<div class="record-menu" role="menu" popover="auto" use:floatMenu>
                <button class="action-button" role="menuitem" onclick={() => request('edit', entry)}>{$translator('common.edit')}</button>
                <button class="action-button" role="menuitem" data-tone="danger" onclick={() => request('delete', entry)}>{$translator('common.delete')}</button>
              </div>{/if}
            </div>
          {/if}
        </div>
        <p class="record-body">{entry.record.body}</p>
      </article>
    {/each}
    {#if $session.page?.next_cursor}<button class="action-button" disabled={busy} onclick={() => session.refresh(true)}>{$translator('taskProgress.more')}</button>{/if}
  </div>
  {#if $session.error}
    <div class="recovery"><p role="alert">{$translator(`taskProgress.error.${$session.error}`)}</p>
      {#if $session.pending}<button class="action-button" disabled={busy} onclick={() => session.retry()}>{$translator('taskProgress.retryOriginal')}</button>{/if}
      <button class="action-button" disabled={busy} onclick={() => session.refresh()}>{$translator('taskProgress.refresh')}</button>
      {#if $session.editing && !$session.pending && $session.error === 'conflict'}<button class="action-button" disabled={busy} onclick={() => session.rebase()}>{$translator('taskProgress.useLatest')}</button>{/if}
    </div>
  {/if}
  {#if !readOnly && (!$session.page || !$session.page.read_only || $session.draft)}
    <footer>
      <label for="progress-body">{$translator($session.editing ? 'taskProgress.edit' : 'taskProgress.add')}</label>
      <textarea id="progress-body" bind:this={input} value={$session.draft} rows={3} maxlength={1000}
        placeholder={$translator('taskProgress.placeholder')} disabled={busy || locked}
        oninput={event => session.setDraft(event.currentTarget.value)}></textarea>
      <div class="editor-actions"><small>{validated.body.length} / 1000</small>
        {#if $session.editing || $session.pending}<button class="action-button" disabled={busy} onclick={() => request('cancel')}>{$translator('common.cancel')}</button>{/if}
        <button class="action-button" data-tone="primary" disabled={busy || locked || !!validated.error || $session.pending?.action === 'delete'} onclick={() => session.save()}>{$translator($session.busy ? 'taskProgress.saving' : 'common.save')}</button>
      </div>
      {#if validated.error && $session.draft.trim()}<p class="status" role="alert">{$translator(`taskProgress.error.${validated.error}`)}</p>{/if}
    </footer>
  {/if}
</dialog>
{#if confirming}
  <dialog class="progress-confirm" bind:this={confirmation} use:showConfirmation aria-label={$translator(confirming === 'delete' ? 'taskProgress.deleteTitle' : 'taskProgress.discardTitle')}
    onkeydown={event => event.stopPropagation()} oncancel={event => { event.preventDefault(); confirming = null; }}>
    <h2>{$translator(confirming === 'delete' ? 'taskProgress.deleteTitle' : 'taskProgress.discardTitle')}</h2>
    <p>{$translator(confirming === 'delete' ? 'taskProgress.deleteHint' : 'taskProgress.discardHint')}</p>
    <div class="confirm-actions"><button class="action-button" onclick={() => confirming = null}>{$translator('common.cancel')}</button>
      <button class="action-button" data-tone="danger" onclick={confirm}>{$translator(confirming === 'delete' ? 'common.delete' : 'taskProgress.discard')}</button></div>
  </dialog>
{/if}

<style>
  dialog { --progress-panel: #fffdf8; --progress-input: #ffffff; --progress-placeholder: #6b6256; background: var(--progress-panel); color: var(--action-text); border: 1px solid var(--action-border); border-radius: 8px; box-sizing: border-box; padding: 14px; font-size: 13px; line-height: 1.5; max-height: calc(100% - 24px); max-width: calc(100% - 24px); }
  :global(html[data-theme='dark']) dialog { --progress-panel: #29251e; --progress-input: #211f1b; --progress-placeholder: #c8bdab; color-scheme: dark; }
  dialog::backdrop { background: #0006; }
  .progress-dialog { width: min(480px, calc(100% - 24px)); }
  .progress-dialog[open] { display: flex; flex-direction: column; gap: 10px; }
  h2 { margin: 0; font-size: 16px; } h3 { margin: 12px 0 6px; font-size: 12px; font-weight: 500; }
  header, .record-meta, .editor-actions, .confirm-actions, .notice { display: flex; align-items: center; gap: 8px; }
  header { justify-content: space-between; } header, footer, .task-title, .recovery, .notice, .status { flex-shrink: 0; }
  .task-title { margin: 0; font-weight: 500; overflow-wrap: anywhere; max-height: 70px; overflow: auto; }
  .entries { overflow: auto; min-height: 0; max-height: 42dvh; scrollbar-width: thin; }
  article { padding: 8px 0; border-bottom: 1px solid var(--action-border); }
  .record-meta { font-size: 11px; } .record-actions { margin-left: auto; }
  .record-actions > button { min-height: 26px; padding: 0 7px; }
  .record-body { white-space: pre-wrap; overflow-wrap: anywhere; margin: 4px 0; }
  .record-menu { position: fixed; inset: auto; margin: 0; padding: 4px; width: 130px; max-width: calc(100vw - 16px); overflow: auto; border: 1px solid var(--action-border); border-radius: 6px; background: var(--progress-panel); color: inherit; }
  .record-menu button { display: block; width: 100%; text-align: left; border: 0; }
  footer { display: block; border-top: 1px solid var(--action-border); padding: 10px 0 0; margin: 0; }
  label { display: block; margin-bottom: 6px; color: var(--action-text); font-size: 12px; font-weight: 500; }
  textarea { width: 100%; box-sizing: border-box; min-width: 0; max-height: 160px; min-height: 70px; resize: vertical; padding: 8px; border: 1px solid var(--action-border); border-radius: 6px; background: var(--progress-input); color: var(--action-text); font: inherit; }
  textarea::placeholder { color: var(--progress-placeholder); opacity: 1; }
  textarea:focus-visible { outline: 2px solid var(--action-focus); outline-offset: 2px; }
  .editor-actions { justify-content: flex-end; flex-wrap: wrap; margin-top: 6px; } small { margin-right: auto; color: var(--progress-placeholder); font-size: 11px; }
  .empty, .status { margin: 4px 0; font-size: 12px; } .recovery p { margin: 0 0 6px; overflow-wrap: anywhere; }
  .recovery button { margin: 0 6px 4px 0; } .notice span { flex: 1; min-width: 0; overflow-wrap: anywhere; }
  .progress-confirm { width: min(360px, calc(100% - 24px)); } .progress-confirm p { overflow-wrap: anywhere; } .confirm-actions { justify-content: flex-end; flex-wrap: wrap; }
  @media (max-height: 450px) { .progress-dialog[open] { display: block; overflow: auto; } .entries { max-height: 180px; } footer { margin-top: 10px; } }
</style>
