<script lang="ts">
  import { onMount, tick } from "svelte";
  import PanelToolButton from "./PanelToolButton.svelte";
  import "./management-dialog.css";
  import { languageState, translator, type TranslationKey } from "$lib/i18n";
  import type { TrashItem } from "$lib/api/trashApi";
  import { createTrashStore } from "$lib/stores/trashStore";
  import { purgeApi, type PurgePlan, type PurgeTarget } from "$lib/api/purgeApi";
  export let onClose: () => void;
  export let afterCommit: () => Promise<void>;
  const store = createTrashStore();
  let dialog: HTMLDialogElement;
  let content: HTMLDivElement;
  let items: TrashItem[] = [];
  let pending: TrashItem | null = null;
  let busy = false;
  let loadFailed = false;
  let more = false;
  let offset = 0;
  let message: TranslationKey | null = null;
  let refreshNeeded = false;
  let disposed = false;
  let selecting = false;
  let selected: string[] = [];
  let purge: PurgePlan | null = null;
  let unfinished: PurgePlan | null = null;
  let agreed = false;
  let stopRequested = false;
  const key = (item: PurgeTarget) => item.kind + ':' + item.uuid;
  function toggle(item: TrashItem) {
    selected = selected.includes(key(item)) ? selected.filter(id => id !== key(item)) : [...selected, key(item)];
  }
  async function preparePurge(targets: PurgeTarget[] | null) {
    if (busy) return;
    busy = true; message = null; agreed = false;
    try { purge = await purgeApi.prepare(targets); }
    catch (error) { message = purgeFailure(error); }
    finally { busy = false; }
  }
  function purgeFailure(error: unknown): TranslationKey {
    const code = error instanceof Error ? error.message : String(error);
    if (code.includes('PURGE_MIGRATION_REQUIRED')) return 'purge.migration';
    if (code.includes('PURGE_PENDING_CREATION')) return 'purge.pendingCreation';
    if (code.includes('PURGE_EMPTY')) return 'purge.empty';
    if (code.includes('PURGE_CONFLICT') || code.includes('PURGE_TARGET_CHANGED')) return 'purge.conflict';
    return 'purge.failed';
  }
  async function runPurge() {
    if (busy || !purge || (purge.state === 'prepared' && !agreed)) return;
    busy = true; stopRequested = false; message = null;
    const operation = purge.operation_uuid;
    try {
      let moreWork: boolean;
      do {
        const previous = purge;
        purge = await purgeApi.run(operation);
        moreWork = purge.pending > 0 || (purge.cleanup_pending > 0 && (previous.pending > 0 || purge.cleanup_pending < previous.cleanup_pending));
      } while (moreWork && !stopRequested);
      unfinished = purge.pending || purge.cleanup_pending ? purge : null;
      selected = []; selecting = false; pending = null;
    } catch (error) { message = purgeFailure(error); }
    finally {
      try { await afterCommit(); } catch { refreshNeeded = true; message = 'purge.refreshFailed'; }
      await page(true); busy = false;
    }
  }

  onMount(() => {
    // Disabling the focused restore button can move focus outside the dialog.
    const escape = (event: KeyboardEvent) => {
      if (dialog.open && event.key === "Escape") {
        event.preventDefault(); event.stopImmediatePropagation(); back();
      }
    };
    window.addEventListener("keydown", escape, true);
    dialog.showModal(); void load(true);
    void purgeApi.unfinished().then(plan => { if (!disposed) unfinished = plan; }).catch(() => { if (!disposed) message = 'purge.failed'; });
    return () => { disposed = true; window.removeEventListener("keydown", escape, true); dialog.close(); };
  });
  function date(value: number) {
    return new Intl.DateTimeFormat($languageState.resolvedLocale, { dateStyle: "medium", timeStyle: "short" }).format(value);
  }
  function listDate(value: number) {
    return new Intl.DateTimeFormat($languageState.resolvedLocale, {
      year: new Date(value).getFullYear() === new Date().getFullYear() ? undefined : "numeric",
      month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
    }).format(value);
  }
  async function page(reset: boolean) {
    loadFailed = false;
    try {
      const next = await store.list(reset ? 0 : offset);
      if (disposed) return;
      const combined = reset ? [] : [...items];
      for (const item of next) {
        const index = combined.findIndex(old => old.kind === item.kind && old.uuid === item.uuid);
        if (index < 0) combined.push(item); else combined[index] = item;
      }
      items = combined; offset = (reset ? 0 : offset) + next.length; more = next.length === 50;
      if (reset) { await tick(); content.scrollTop = 0; }
    } catch { if (!disposed) loadFailed = true; }
  }
  async function load(reset: boolean) {
    if (busy) return;
    busy = true;
    try {
      if (refreshNeeded) {
        try { await afterCommit(); refreshNeeded = false; message = null; }
        catch { message = "trash.refreshFailed"; }
      }
      await page(reset);
    } finally { busy = false; }
  }
  function failure(error: unknown): TranslationKey {
    const code = error instanceof Error ? error.message : String(error);
    if (code === "TRASH_CONFLICT" || code === "TRASH_NOT_FOUND") return "trash.conflict";
    if (code === "TRASH_RULE_ACTIVE") return "trash.ruleActive";
    if (code === "TRASH_INVALID_VERSION") return "trash.invalidVersion";
    return "trash.changeFailed";
  }
  async function select(item: TrashItem) {
    if (busy) return;
    busy = true; message = null;
    try { pending = await store.preview(item); await tick(); content.scrollTop = 0; }
    catch (error) { message = failure(error); }
    finally { busy = false; }
  }
  async function confirm() {
    if (busy || !pending) return;
    busy = true; message = null;
    try {
      refreshNeeded = await store.restore(pending, afterCommit);
      message = refreshNeeded ? "trash.refreshFailed" : "trash.restored";
    } catch (error) { message = failure(error); }
    finally { pending = null; await page(true); busy = false; }
  }
  function back() {
    if (busy) return;
    if (purge) { purge = null; agreed = false; }
    else if (pending) pending = null;
    else if (selecting) { selecting = false; selected = []; }
    else onClose();
  }
</script>

<dialog class="management-dialog" bind:this={dialog} aria-labelledby="trash-heading" onkeydown={event => {
  event.stopPropagation();
  if (event.key === "Escape") { event.preventDefault(); back(); }
}}
  oncancel={event => { event.preventDefault(); back(); }}>
  <header><h2 id="trash-heading">{$translator(purge ? "purge.title" : pending ? "trash.preview" : "trash.title")}</h2>
    {#if !pending && !purge}
      <div class="header-tools">
        <PanelToolButton icon="refresh" label={$translator("trash.refresh")} disabled={busy} onclick={() => load(true)} />
        <PanelToolButton icon="close" label={$translator("common.close")} disabled={busy} onclick={back} />
      </div>
    {/if}
  </header>
  {#if !pending && !purge && items.length}
    <div class="purge-toolbar">
      <button class="text-tool" disabled={busy || loadFailed} onclick={() => { selecting = !selecting; selected = []; }}>{$translator(selecting ? 'common.cancel' : 'purge.select')}</button>
      {#if selecting}<span>{$translator('purge.selected', {count:selected.length})}</span>{/if}
      <button class="text-tool" disabled={busy || loadFailed} onclick={() => preparePurge(null)}>{$translator('purge.all')}</button>
    </div>
  {/if}
  {#if unfinished && !purge}<button class="text-tool" disabled={busy} onclick={() => { purge = unfinished; agreed = true; }}>{$translator('purge.resume')}</button>{/if}
  <div class="content" bind:this={content} aria-busy={busy}>
    {#if message}<p role="status">{$translator(message)}</p>{/if}
    {#if busy}<p role="status">{$translator("common.loading")}</p>{/if}
    {#if loadFailed}<p role="alert">{$translator("trash.loadFailed")}</p>{/if}
    {#if purge}
      <h3>{$translator('purge.count', {count:purge.total,attachments:purge.attachments})}</h3>
      {#if purge.state === 'prepared'}
        <p>{$translator('purge.warning')}</p>
        <label class="purge-agreement"><input type="checkbox" bind:checked={agreed} disabled={busy} /> <span>{$translator('purge.confirm')}</span></label>
      {:else}
        <p role="status">{$translator('purge.progress',{done:purge.total-purge.pending,total:purge.total})}</p>
        <progress max={purge.total} value={purge.total-purge.pending} aria-label={$translator('purge.title')}></progress>
        <p>{$translator('purge.result',{purged:purge.purged,skipped:purge.skipped})}</p>
        {#if purge.skipped}<p>{$translator('purge.skipped')}</p>{/if}
        {#if purge.cleanup_pending}<p role="status">{$translator('purge.cleanup',{count:purge.cleanup_pending})}</p>{/if}
      {/if}
    {:else if pending}
      <h3>{pending.title || $translator("trash.untitled")}</h3>
      <p class="meta">{$translator(pending.kind === "todo" ? "trash.todo" : "trash.note")} · {$translator("trash.deleted")}: {date(pending.deleted_at)}</p>
      {#if pending.kind === "todo"}
        <p>{$translator(pending.completed ? "trash.completed" : "trash.incomplete")}</p>
        <p>{$translator("trash.taskHint")}</p>
      {/if}
      <p>{$translator("trash.linkHint")}</p>
      {#if pending.attachments.length}
        <p>{$translator("trash.attachmentHint")}</p>
        <ul class="attachments">{#each pending.attachments as attachment (attachment.uuid)}<li>{attachment.name}</li>{/each}</ul>
      {/if}
      <p class="body">{pending.content}</p>
    {:else}
      {#if !busy && !loadFailed && !items.length}<p>{$translator("trash.empty")}</p>{/if}
      <ul class="records">
        {#each items as item (item.kind + item.uuid)}
          <li class:selected={selecting && selected.includes(key(item))}>
            {#if selecting}<label class="selection-check"><input type="checkbox" checked={selected.includes(key(item))} aria-label={item.title || $translator('trash.untitled')} disabled={busy} onchange={() => toggle(item)} /></label>{/if}
            <button class="record" disabled={busy || loadFailed} onclick={() => selecting ? toggle(item) : select(item)}>
            <strong>{item.title || $translator("trash.untitled")}</strong>
            <span class="meta">{$translator(item.kind === "todo" ? "trash.todo" : "trash.note")} · {listDate(item.deleted_at)}</span>
            {#if item.content}<span class="excerpt">{item.content}</span>{/if}
          </button></li>
        {/each}
      </ul>
      {#if more}<button class="action-button" disabled={busy || loadFailed} onclick={() => load(false)}>{$translator("trash.more")}</button>{/if}
    {/if}
  </div>
  {#if purge}
    <footer class="batch-actions">
      <button class="action-button" onclick={() => busy ? stopRequested = true : back()} disabled={busy && stopRequested}>{$translator(busy ? 'purge.pause' : 'common.close')}</button>
      {#if purge.pending || purge.cleanup_pending}<button class="action-button" data-tone="danger" disabled={busy || (purge.state === 'prepared' && !agreed)} onclick={runPurge}>{$translator(purge.state === 'prepared' ? 'purge.title' : 'common.retry')}</button>{/if}
    </footer>
  {:else if selecting}
    <footer><button class="action-button" data-tone="danger" disabled={busy || !selected.length} onclick={() => preparePurge(items.filter(item => selected.includes(key(item))).map(({kind,uuid}) => ({kind,uuid})))}>{$translator('purge.title')}</button></footer>
  {:else if pending}
    <footer>
      <button class="action-button" disabled={busy} onclick={back}>{$translator("common.cancel")}</button>
      <button class="action-button" data-tone="danger" disabled={busy} onclick={() => pending && preparePurge([{kind:pending.kind,uuid:pending.uuid}])}>{$translator('purge.title')}</button>
      <button class="action-button" data-tone="primary" disabled={busy} onclick={confirm}>{$translator("trash.restore")}</button>
    </footer>
  {/if}
</dialog>

<style>
  dialog { width: min(600px, calc(100% - 24px)); max-height: calc(100% - 24px); box-sizing: border-box; border: 1px solid #d5c8ac; border-radius: 8px; background: #fffaf0; color: #463e31; }
  dialog[open] { display: flex; flex-direction: column; }
  dialog::backdrop { background: #0006; }
  h2 { margin: 0; font-size: 18px; } h3 { margin: 0 0 8px; font-size: 16px; overflow-wrap: anywhere; }
  p { margin: 8px 0; font-size: 14px; overflow-wrap: anywhere; }
  .content { min-height: 0; overflow: auto; }
  .body { white-space: pre-wrap; }
  .purge-toolbar { display:flex; align-items:center; gap:8px; flex-wrap:wrap; font-size:13px; }
  .purge-toolbar button:last-child { margin-left:auto; }
  .purge-agreement { display:flex; align-items:flex-start; gap:8px; font-size:14px; margin:16px 0; }
  .purge-agreement input { margin-top:3px; }
  progress { width:100%; height:6px; accent-color:#b88c16; }
  .attachments { padding-left: 20px; overflow-wrap: anywhere; font-size: 14px; }
  footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  :global(html[data-theme="dark"]) dialog { background: #302d27; color: #f4e7cd; border-color: #6a5842; color-scheme: dark; }
</style>
