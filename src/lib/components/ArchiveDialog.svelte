<script lang="ts">
  import { onMount, tick } from "svelte";
  import { languageState, translator, type TranslationKey } from "$lib/i18n";
  import { createArchiveStore } from "$lib/stores/archiveStore";
  import { createArchiveBatchStore } from "$lib/stores/archiveBatchStore";
  import type { ArchiveAction, ArchivePreview, ArchiveRequest, ArchiveCursor, ArchiveBatchRequest, ArchiveBatchAction, ArchiveJob } from "$lib/api/archiveApi";
  export let onClose: () => void;
  export let afterCommit: () => Promise<void>;
  export let onViewTask: (uuid: string) => Promise<void>;
  const store = createArchiveStore();
  const batches = createArchiveBatchStore();
  let selecting = false, selected: ArchivePreview[] = [], jobs: ArchiveJob[] = [];
  let batchRequest: ArchiveBatchRequest | null = null, job: ArchiveJob | null = null;
  let batchStarted = false, stopConfirm = false;
  $: batchComplete = job !== null && job.results.length === job.targets.length;
  let dialog: HTMLDialogElement;
  let content: HTMLDivElement;
  let items: ArchivePreview[] = [];
  let pending: ArchivePreview | null = null;
  let request: ArchiveRequest | null = null;
  let cursor: ArchiveCursor | null = null;
  let query = "", activeQuery = "", busy = false, loadFailed = false, blocked = false, disposed = false;
  let refreshNeeded = false, lastUuid = "", groupReset = false;
  let message: TranslationKey | null = null;
  interface Item { uuid: string; content: string; completed: boolean; deleted_at: number | null; sort_order: number; }
  let checklist: Item[] = [];
  $: checklist = pending ? (JSON.parse(pending.checklist_json) as { items: Item[] }).items
    .filter(item => item.deleted_at === null).sort((a,b) => a.sort_order - b.sort_order || a.uuid.localeCompare(b.uuid)) : [];
  const label = (action: ArchiveAction): TranslationKey => action === "delete" ? "archive.delete" : action === "reopen" ? "archive.reopen" : "archive.unarchive";
  const hint = (action: ArchiveAction): TranslationKey => action === "delete" ? "archive.deleteHint" : action === "reopen" ? "archive.reopenHint" : "archive.unarchiveHint";
  function date(value: number) {
    return new Intl.DateTimeFormat($languageState.resolvedLocale, { dateStyle: "medium", timeStyle: "short" }).format(value);
  }
  onMount(() => {
    const escape = (event: KeyboardEvent) => {
      if (dialog.open && event.key === "Escape") { event.preventDefault(); event.stopImmediatePropagation(); back(); }
    };
    window.addEventListener("keydown", escape, true);
    dialog.showModal(); void load(true);
    return () => { disposed = true; window.removeEventListener("keydown", escape, true); dialog.close(); };
  });
  function back() {
    if (busy) return;
    if (stopConfirm) { stopConfirm = false; }
    else if (batchRequest) {
      const started = batchStarted;
      batchRequest = null; job = null; batchStarted = false; blocked = false;
      if (started) { selecting = false; selected = []; void load(true); }
    }
    else if (selecting) { selecting = false; selected = []; }
    else if (request) { request = null; }
    else if (pending) { pending = null; blocked = false; }
    else onClose();
  }
  async function page(reset: boolean) {
    loadFailed = false;
    try {
      const next = await store.list(activeQuery, reset ? null : cursor);
      const pendingJobs = await batches.pending();
      if (disposed) return;
      const combined = reset ? [] : [...items];
      for (const item of next.items) {
        const index = combined.findIndex(old => old.expected.uuid === item.expected.uuid);
        if (index < 0) combined.push(item); else combined[index] = item;
      }
      items = combined; cursor = next.next;
      jobs = pendingJobs;
      if (reset) { await tick(); if (content) content.scrollTop = 0; }
    } catch { if (!disposed) loadFailed = true; }
  }
  async function load(reset: boolean) {
    if (busy) return;
    busy = true;
    if (reset) { activeQuery = query; cursor = null; if (!batchRequest) selected = []; }
    try {
      if (refreshNeeded) {
        try { await afterCommit(); refreshNeeded = false; message = null; }
        catch { message = "archive.batchRefreshFailed"; }
      }
      await page(reset);
    } finally { busy = false; }
  }
  function failure(error: unknown): TranslationKey {
    const code = error instanceof Error ? error.message : String(error);
    if (["ARCHIVE_CONFLICT", "ARCHIVE_NOT_ARCHIVED", "ARCHIVE_NOT_FOUND"].includes(code)) { blocked = true; return "archive.conflict"; }
    if (code === "ARCHIVE_SCOPE_CHANGED") { blocked = true; return "archive.scopeChanged"; }
    if (code === "ARCHIVE_RULE_ACTIVE") { blocked = true; return "archive.ruleActive"; }
    if (code === "ARCHIVE_BATCH_ENDED") { blocked = true; return "archive.batchStopped"; }
    return "archive.changeFailed";
  }
  async function select(item: ArchivePreview) {
    if (busy || refreshNeeded) return;
    busy = true; blocked = false; message = null; request = null; groupReset = false; lastUuid = "";
    try { pending = await store.preview(item); await tick(); content.scrollTop = 0; }
    catch (error) { message = failure(error); }
    finally { busy = false; }
  }
  function choose(action: ArchiveAction) {
    if (busy || blocked || !pending) return;
    request = store.prepare(action, pending); message = null;
    content.scrollTop = 0;
  }
  async function confirm() {
    if (busy || !request || blocked) return;
    busy = true; message = null;
    try {
      const result = await store.apply(request, afterCommit);
      refreshNeeded = result.refreshNeeded;
      groupReset = result.outcome.warnings.includes("GROUP_RESET");
      lastUuid = request.action === "delete" ? "" : result.outcome.uuid;
      request = null; pending = null;
      message = refreshNeeded ? "archive.refreshFailed" : "archive.saved";
      await page(true);
    } catch (error) { message = failure(error); }
    finally { busy = false; }
  }
  async function viewTask() {
    if (busy) return; busy = true;
    try { await onViewTask(lastUuid); }
    catch { message = "archive.refreshFailed"; refreshNeeded = true; }
    finally { busy = false; }
  }
  function toggle(item: ArchivePreview) {
    if (busy || loadFailed || refreshNeeded) return;
    selected = selected.some(v => v.expected.uuid === item.expected.uuid)
      ? selected.filter(v => v.expected.uuid !== item.expected.uuid) : [...selected, structuredClone(item)];
  }
  async function chooseBatch(action: ArchiveBatchAction) {
    if (busy || !selected.length || refreshNeeded) return;
    busy = true; message = null; blocked = false;
    try {
      const previews: ArchivePreview[] = [];
      for (const item of selected) previews.push(await store.preview(item));
      selected = previews; batchRequest = batches.prepare(action, previews);
      job = null; batchStarted = false; stopConfirm = false; content.scrollTop = 0;
    } catch (error) { message = failure(error); }
    finally { busy = false; }
  }
  function resumeBatch(value: ArchiveJob) {
    job = structuredClone(value);
    batchRequest = { operation: value.operation_uuid, action: value.action, targets: structuredClone(value.targets) };
    batchStarted = true; stopConfirm = false; blocked = false; message = null; selected = [];
    content.scrollTop = 0;
  }
  async function runBatch() {
    if (busy || blocked || !batchRequest) return;
    busy = true; batchStarted = true; message = null;
    try {
      const result = await batches.execute(batchRequest, afterCommit, value => { job = value; });
      if (result.job) job = result.job;
      refreshNeeded = result.refreshNeeded;
      message = result.error ? failure(result.error) : refreshNeeded ? "archive.batchRefreshFailed" : null;
      await page(true);
    } catch (error) { message = failure(error); refreshNeeded = true;
    } finally { busy = false; }
  }
  async function finishBatch() {
    if (busy || !batchRequest) return;
    if (!batchComplete && !stopConfirm) { stopConfirm = true; content.scrollTop = 0; return; }
    busy = true;
    try {
      await batches.dismiss(batchRequest.operation);
      batchRequest = null; job = null; batchStarted = false; stopConfirm = false; selecting = false; selected = []; blocked = false;
      await page(true);
    } catch { message = "archive.changeFailed"; }
    finally { busy = false; }
  }
  function resultLabel(error: string | null): TranslationKey {
    return error === null ? "archive.batchApplied" : error === "ARCHIVE_RULE_ACTIVE" ? "archive.ruleActive" : "archive.batchChanged";
  }
</script>

<dialog bind:this={dialog} aria-labelledby="archive-heading"
  onkeydown={event => event.stopPropagation()} oncancel={event => { event.preventDefault(); back(); }}>
  <header><h2 id="archive-heading">{$translator(batchRequest ? "archive.batchTitle" : request ? label(request.action) : "archive.title")}</h2>
    {#if !pending && !batchRequest && !selecting}<button class="action-button" disabled={busy} onclick={back}>{$translator("common.close")}</button>{/if}</header>
  <div class="content" bind:this={content} aria-busy={busy}>
    {#if message}<p role="status">{$translator(message)}</p>{/if}
    {#if groupReset}<p role="status">{$translator("archive.groupReset")}</p>{/if}
    {#if busy}<p role="status">{$translator("common.loading")}</p>{/if}
    {#if batchRequest}
      <h3>{$translator(label(batchRequest.action))} · {batchRequest.targets.length}</h3>
      <p>{$translator(hint(batchRequest.action))}</p>
      {#if stopConfirm}<p role="alert">{$translator("archive.batchStopHint")}</p>
      {:else if !batchStarted}
        <p>{$translator("archive.batchFixed")}</p>
        <ul>{#each selected as item (item.expected.uuid)}<li>{item.title}</li>{/each}</ul>
      {:else}
        <p role="status">{$translator("archive.batchProgress", { done: job?.results.length ?? 0, total: batchRequest.targets.length })}</p>
        <p>{$translator("archive.batchSummary", { success: job?.results.filter(v => !v.error).length ?? 0, skipped: job?.results.filter(v => v.error).length ?? 0 })}</p>
        {#if batchComplete}<p>{$translator("archive.batchComplete")}</p>{/if}
        <ul class="batch-results">{#each job?.results ?? [] as result (result.uuid)}
          <li><strong>{selected.find(v => v.expected.uuid === result.uuid)?.title ?? result.uuid}</strong>
            <span>{$translator(resultLabel(result.error))}</span>
            {#if result.result?.warnings.includes("GROUP_RESET")}<span>{$translator("archive.groupReset")}</span>{/if}</li>
        {/each}</ul>
      {/if}
      {#if refreshNeeded}<p role="alert">{$translator("archive.batchRefreshFailed")}</p>{/if}
    {:else if pending}
      <h3>{pending.title}</h3>
      <p class="meta">{$translator(pending.completed ? "trash.completed" : "trash.incomplete")}
        {#if pending.group_name} · {pending.group_name}{/if}</p>
      <p class="meta">{$translator("archive.archivedOn")}: {date(pending.archived_at)}</p>
      {#if pending.due_date || pending.due_at !== null}<p class="meta">{$translator("archive.due")}: {pending.due_date ?? date(pending.due_at!)}</p>{/if}
      {#if request}<p class="confirmation">{$translator(hint(request.action))}</p>{/if}
      {#if pending.content}<p class="body">{pending.content}</p>{/if}
      {#if checklist.length}
        <h4>{$translator("archive.checklist")} {checklist.filter(item => item.completed).length}/{checklist.length}</h4>
        <ul class="checklist">{#each checklist as item (item.uuid)}
          <li><input type="checkbox" checked={item.completed} disabled aria-label={item.content} /><span>{item.content}</span></li>
        {/each}</ul>
      {/if}
    {:else}
      <form onsubmit={event => { event.preventDefault(); void load(true); }}>
        <input aria-label={$translator("archive.search")} placeholder={$translator("archive.search")} maxlength="200" bind:value={query} disabled={busy} />
        <button class="action-button" disabled={busy}>{$translator("contentSearch.search")}</button>
      </form>
      {#if !selecting}
        {#each jobs as saved (saved.operation_uuid)}
          <button class="record" disabled={busy || loadFailed} onclick={() => resumeBatch(saved)}>
            <strong>{$translator("archive.batchResume")}: {$translator(label(saved.action))}</strong>
            <span>{saved.results.length} / {saved.targets.length}</span>
          </button>
        {/each}
        <button class="action-button" disabled={busy || loadFailed || refreshNeeded || !items.length || jobs.length > 0}
          onclick={() => { selecting = true; selected = []; message = null; }}>{$translator("archive.batchSelect")}</button>
      {:else}
        <div class="selection-tools"><span>{$translator("archive.batchSelected", { count: selected.length })}</span>
          <button class="action-button" disabled={busy || loadFailed || refreshNeeded} onclick={() => { selected = structuredClone(items); }}>{$translator("archive.batchSelectLoaded")}</button>
          <button class="action-button" disabled={busy} onclick={() => { selected = []; }}>{$translator("common.clear")}</button></div>
      {/if}
      {#if loadFailed}<p role="alert">{$translator("archive.loadFailed")}</p>{/if}
      {#if !busy && !loadFailed && !items.length}<p class="empty">{$translator("archive.empty")}</p>{/if}
      <ul class="records">{#each items as item (item.expected.uuid)}
        <li class:selected={selecting && selected.some(v => v.expected.uuid === item.expected.uuid)}>
        <button class="record" aria-pressed={selecting ? selected.some(v => v.expected.uuid === item.expected.uuid) : undefined}
          disabled={busy || loadFailed || refreshNeeded} onclick={() => selecting ? toggle(item) : select(item)}>
          {#if selecting}<input type="checkbox" tabindex="-1" checked={selected.some(v => v.expected.uuid === item.expected.uuid)} aria-hidden="true" />{/if}
          <strong>{item.title}</strong>
          <span class="meta">{$translator(item.completed ? "trash.completed" : "trash.incomplete")} · {date(item.archived_at)}</span>
          {#if item.content}<span class="excerpt">{item.content}</span>{/if}
        </button></li>
      {/each}</ul>
      {#if cursor}<button class="action-button" disabled={busy || loadFailed} onclick={() => load(false)}>{$translator("trash.more")}</button>{/if}
    {/if}
  </div>
  <footer>
    {#if batchRequest}
      {#if stopConfirm}
        <button class="action-button" disabled={busy} onclick={back}>{$translator("common.cancel")}</button>
        <button class="action-button" data-tone="danger" disabled={busy} onclick={finishBatch}>{$translator("archive.batchStop")}</button>
      {:else if !batchStarted}
        <button class="action-button" disabled={busy} onclick={back}>{$translator("common.cancel")}</button>
        <button class="action-button" data-tone={batchRequest.action === "delete" ? "danger" : "primary"} disabled={busy} onclick={runBatch}>{$translator("archive.confirm")}</button>
      {:else}
        <button class="action-button" disabled={busy} onclick={back}>{$translator("archive.back")}</button>
        {#if refreshNeeded}<button class="action-button" disabled={busy} onclick={() => load(true)}>{$translator("archive.refresh")}</button>{/if}
        {#if !batchComplete && !blocked}<button class="action-button" data-tone="primary" disabled={busy} onclick={runBatch}>{$translator("archive.batchContinue")}</button>{/if}
        <button class="action-button" disabled={busy || (batchComplete && refreshNeeded)} onclick={finishBatch}>{$translator(batchComplete ? "common.done" : "archive.batchStop")}</button>
      {/if}
    {:else if selecting}
      <button class="action-button" disabled={busy} onclick={back}>{$translator("common.cancel")}</button>
      <button class="action-button" data-tone="danger" disabled={busy || !selected.length || refreshNeeded} onclick={() => chooseBatch("delete")}>{$translator("archive.delete")}</button>
      <button class="action-button" data-tone="primary" disabled={busy || !selected.length || refreshNeeded} onclick={() => chooseBatch("unarchive")}>{$translator("archive.unarchive")}</button>
    {:else if request}
      <button class="action-button" disabled={busy} onclick={back}>{$translator("common.cancel")}</button>
      <button class="action-button" data-tone={request.action === "delete" ? "danger" : "primary"} disabled={busy || blocked} onclick={confirm}>{$translator("archive.confirm")}</button>
    {:else if pending}
      <button class="action-button" disabled={busy} onclick={back}>{$translator("archive.back")}</button>
      <button class="action-button" data-tone="danger" disabled={busy || blocked} onclick={() => choose("delete")}>{$translator("archive.delete")}</button>
      <button class="action-button" disabled={busy || blocked} onclick={() => choose("reopen")}>{$translator("archive.reopen")}</button>
      <button class="action-button" data-tone="primary" disabled={busy || blocked} onclick={() => choose("unarchive")}>{$translator("archive.unarchive")}</button>
    {:else}
      {#if lastUuid && !refreshNeeded}<button class="action-button" disabled={busy} onclick={viewTask}>{$translator("archive.viewTask")}</button>{/if}
      <button class="action-button" disabled={busy} onclick={() => load(true)}>{$translator("archive.refresh")}</button>
    {/if}
  </footer>
</dialog>
<style>
  dialog { width: min(600px, calc(100% - 24px)); max-height: calc(100% - 24px); box-sizing: border-box; padding: 16px; border: 1px solid #d5c8ac; border-radius: 8px; background: #fffaf0; color: #463e31; font-size: 14px; }
  dialog[open] { display: flex; flex-direction: column; gap: 12px; }
  dialog::backdrop { background: #0006; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  h2 { margin: 0; font-size: 18px; } h3 { margin: 0; font-size: 16px; overflow-wrap: anywhere; }
  h4 { font-size: 14px; margin: 16px 0 8px; }
  p { margin: 8px 0; overflow-wrap: anywhere; }
  .content { min-height: 0; overflow: auto; }
  .body { white-space: pre-wrap; }
  form { display: flex; gap: 8px; margin-bottom: 12px; }
  form input { flex: 1; min-width: 0; border: 1px solid #998969; border-radius: 8px; padding: 10px; font: inherit; color: inherit; background: transparent; }
  form input:focus-visible { outline: 2px solid #a47e13; outline-offset: -2px; }
  .records { margin: 0; padding: 0; list-style: none; }
  .record { display: flex; flex-direction: column; gap: 6px; width: 100%; padding: 12px 4px; border: 0; border-bottom: 1px solid #9c8a6333; background: transparent; color: inherit; text-align: left; cursor: pointer; font: inherit; overflow-wrap: anywhere; }
  .record:hover:not(:disabled) { background: #9c8a6314; }
  .record strong { font-size: 14px; font-weight: 500; }
  .record:disabled { opacity: .6; cursor: default; }
  .meta, .excerpt { font-size: 12px; opacity: .85; }
  .excerpt { display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; white-space: pre-wrap; }
  .confirmation { padding: 12px 0; border-block: 1px solid #9c8a6355; }
  .checklist { list-style: none; padding: 0; }
  .checklist li { display: flex; gap: 8px; align-items: flex-start; margin: 10px 0; overflow-wrap: anywhere; }
  .checklist input { flex: none; margin: 3px 0; }
  .empty { padding: 16px 0; }
  .selection-tools { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin: 12px 0; }
  .selected { background: #c59a2630; }
  .record input { pointer-events: none; margin: 0; }
  .batch-results { list-style: none; padding: 0; }
  .batch-results li { display: flex; flex-direction: column; gap: 4px; padding: 8px 0; overflow-wrap: anywhere; }
  .batch-results strong { font-weight: 500; } .batch-results span { font-size: 12px; }
  footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  :global(html[data-theme="dark"]) dialog { background: #302d27; color: #f4e7cd; border-color: #6a5842; color-scheme: dark; }
</style>
