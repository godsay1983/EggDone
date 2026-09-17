<script lang="ts">
  import { onMount, tick } from "svelte";
  import { languageState, translator, type TranslationKey } from "$lib/i18n";
  import { createArchiveStore } from "$lib/stores/archiveStore";
  import type { ArchiveAction, ArchivePreview, ArchiveRequest, ArchiveCursor } from "$lib/api/archiveApi";
  export let onClose: () => void;
  export let afterCommit: () => Promise<void>;
  export let onViewTask: (uuid: string) => Promise<void>;
  const store = createArchiveStore();
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
    if (request) { request = null; }
    else if (pending) { pending = null; blocked = false; }
    else onClose();
  }
  async function page(reset: boolean) {
    loadFailed = false;
    try {
      const next = await store.list(activeQuery, reset ? null : cursor);
      if (disposed) return;
      const combined = reset ? [] : [...items];
      for (const item of next.items) {
        const index = combined.findIndex(old => old.expected.uuid === item.expected.uuid);
        if (index < 0) combined.push(item); else combined[index] = item;
      }
      items = combined; cursor = next.next;
      if (reset) { await tick(); if (content) content.scrollTop = 0; }
    } catch { if (!disposed) loadFailed = true; }
  }
  async function load(reset: boolean) {
    if (busy) return;
    busy = true;
    if (reset) { activeQuery = query; cursor = null; }
    try {
      if (refreshNeeded) {
        try { await afterCommit(); refreshNeeded = false; message = "archive.saved"; }
        catch { message = "archive.refreshFailed"; }
      }
      await page(reset);
    } finally { busy = false; }
  }
  function failure(error: unknown): TranslationKey {
    const code = error instanceof Error ? error.message : String(error);
    if (["ARCHIVE_CONFLICT", "ARCHIVE_NOT_ARCHIVED", "ARCHIVE_NOT_FOUND"].includes(code)) { blocked = true; return "archive.conflict"; }
    if (code === "ARCHIVE_SCOPE_CHANGED") { blocked = true; return "archive.scopeChanged"; }
    if (code === "ARCHIVE_RULE_ACTIVE") { blocked = true; return "archive.ruleActive"; }
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
</script>

<dialog bind:this={dialog} aria-labelledby="archive-heading"
  onkeydown={event => event.stopPropagation()} oncancel={event => { event.preventDefault(); back(); }}>
  <header><h2 id="archive-heading">{$translator(request ? label(request.action) : "archive.title")}</h2>
    {#if !pending}<button class="action-button" disabled={busy} onclick={back}>{$translator("common.close")}</button>{/if}</header>
  <div class="content" bind:this={content} aria-busy={busy}>
    {#if message}<p role="status">{$translator(message)}</p>{/if}
    {#if groupReset}<p role="status">{$translator("archive.groupReset")}</p>{/if}
    {#if busy}<p role="status">{$translator("common.loading")}</p>{/if}
    {#if pending}
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
      {#if loadFailed}<p role="alert">{$translator("archive.loadFailed")}</p>{/if}
      {#if !busy && !loadFailed && !items.length}<p class="empty">{$translator("archive.empty")}</p>{/if}
      <ul class="records">{#each items as item (item.expected.uuid)}
        <li><button class="record" disabled={busy || loadFailed || refreshNeeded} onclick={() => select(item)}>
          <strong>{item.title}</strong>
          <span class="meta">{$translator(item.completed ? "trash.completed" : "trash.incomplete")} · {date(item.archived_at)}</span>
          {#if item.content}<span class="excerpt">{item.content}</span>{/if}
        </button></li>
      {/each}</ul>
      {#if cursor}<button class="action-button" disabled={busy || loadFailed} onclick={() => load(false)}>{$translator("trash.more")}</button>{/if}
    {/if}
  </div>
  <footer>
    {#if request}
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
  footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  :global(html[data-theme="dark"]) dialog { background: #302d27; color: #f4e7cd; border-color: #6a5842; color-scheme: dark; }
</style>
