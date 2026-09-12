<script lang="ts">
  import { onMount, tick } from "svelte";
  import { languageState, translator, type TranslationKey } from "$lib/i18n";
  import type { HistoryPreview, HistorySummary } from "$lib/api/noteHistoryApi";
  import { createNoteHistoryStore } from "$lib/stores/noteHistoryStore";
  export let uuid: string;
  export let beforeRestore: () => void;
  export let afterRestore: (changed: boolean) => Promise<void>;
  export let onRefresh: () => Promise<void>;
  export let onClose: (refreshFailed: boolean) => void;
  const store = createNoteHistoryStore();
  let dialog: HTMLDialogElement;
  let content: HTMLDivElement;
  let items: HistorySummary[] = [];
  let preview: HistoryPreview | null = null;
  let confirming = false;
  let busy = false;
  let loadFailed = false;
  let message: TranslationKey | null = null;
  let refreshNeeded = false;
  let disposed = false;

  onMount(() => {
    const escape = (event: KeyboardEvent) => {
      if (dialog.open && event.key === "Escape") {
        event.preventDefault(); event.stopImmediatePropagation(); back();
      }
    };
    window.addEventListener("keydown", escape, true);
    dialog.showModal(); void load();
    return () => { disposed = true; window.removeEventListener("keydown", escape, true); dialog.close(); };
  });
  function date(value: number) {
    return new Intl.DateTimeFormat($languageState.resolvedLocale, { dateStyle: "medium", timeStyle: "short" }).format(value);
  }
  async function list() {
    loadFailed = false;
    try { const next = await store.list(uuid); if (!disposed) items = next; }
    catch { if (!disposed) loadFailed = true; }
  }
  async function load() {
    if (busy) return;
    busy = true; preview = null; confirming = false;
    try {
      if (refreshNeeded) {
        try { await onRefresh(); refreshNeeded = false; message = "history.refreshed"; }
        catch { message = "history.refreshFailed"; }
      } else message = null;
      await list();
    } finally { busy = false; }
  }
  function failure(error: unknown): TranslationKey {
    const code = error instanceof Error ? error.message : String(error);
    if (code === "NOTE_HISTORY_CONFLICT" || code === "NOTE_HISTORY_NOT_FOUND") return "history.conflict";
    if (code === "NOTE_HISTORY_NOTE_UNAVAILABLE") return "history.unavailable";
    if (code === "NOTE_HISTORY_INVALID_VERSION" || code === "NOTE_HISTORY_INVALID_TEXT") return "history.invalid";
    return "history.failed";
  }
  async function select(item: HistorySummary) {
    if (busy || refreshNeeded) return;
    busy = true; message = null; confirming = false;
    try { preview = await store.preview(uuid, item.id); await tick(); content.scrollTop = 0; }
    catch (error) { message = failure(error); }
    finally { busy = false; }
  }
  async function restore() {
    if (busy || !preview || !confirming || refreshNeeded) return;
    busy = true;
    try {
      const result = await store.restore(preview, beforeRestore, afterRestore);
      refreshNeeded = result.refreshFailed;
      message = refreshNeeded ? "history.refreshFailed" : result.changed ? "history.restored" : "history.unchanged";
    } catch (error) { message = failure(error); }
    finally { preview = null; confirming = false; await list(); busy = false; }
  }
  async function beginConfirmation() {
    confirming = true;
    await tick(); content.scrollTop = 0;
  }
  function back() {
    if (busy) return;
    if (confirming) confirming = false;
    else if (preview) preview = null;
    else onClose(refreshNeeded);
  }
</script>

<dialog bind:this={dialog} aria-labelledby="history-heading"
  oncancel={event => { event.preventDefault(); back(); }}>
  <header><h2 id="history-heading">{$translator("history.title")}</h2></header>
  <div class="content" bind:this={content} aria-busy={busy}>
    {#if !preview}<p class="scope">{$translator("history.scope")}</p>{/if}
    {#if message}<p role="status">{$translator(message)}</p>{/if}
    {#if busy}<p role="status">{$translator("common.loading")}</p>{/if}
    {#if loadFailed}<p role="alert">{$translator("history.loadFailed")}</p>{/if}
    {#if preview}
      {#if confirming}<p role="status">{$translator("history.confirmHint")}</p>{/if}
      <p>{$translator("history.captured")}: {date(preview.entry.captured_at)}</p>
      <div class="comparison">
        <section aria-label={$translator("history.current")}>
          <h3>{$translator("history.current")}</h3>
          <strong>{preview.current.title || $translator("trash.untitled")}</strong>
          <p class="body">{preview.current.content}</p>
        </section>
        <section aria-label={$translator("history.selected")}>
          <h3>{$translator("history.selected")}</h3>
          <strong>{preview.entry.text.title || $translator("trash.untitled")}</strong>
          <p class="body">{preview.entry.text.content}</p>
        </section>
      </div>
    {:else}
      {#if !busy && !loadFailed && !items.length}<p>{$translator("history.empty")}</p>{/if}
      <ul class="records">
        {#each items as item (item.id)}
          <li><button class="record action-button" disabled={busy || loadFailed || refreshNeeded} onclick={() => select(item)}>
            <strong>{item.title || $translator("trash.untitled")}</strong>
            <span>{date(item.captured_at)}</span><span class="excerpt">{item.excerpt}</span>
          </button></li>
        {/each}
      </ul>
    {/if}
  </div>
  <footer>
    <button class="action-button" disabled={busy} onclick={back}>{$translator(preview ? "common.cancel" : "common.close")}</button>
    {#if preview}
      {#if confirming}
        <button class="action-button" data-tone="primary" disabled={busy} onclick={restore}>{$translator("history.confirm")}</button>
      {:else}
        <button class="action-button" disabled={busy} onclick={beginConfirmation}>{$translator("history.restore")}</button>
      {/if}
    {:else}
      <button class="action-button" disabled={busy} onclick={load}>{$translator("trash.refresh")}</button>
    {/if}
  </footer>
</dialog>

<style>
  dialog { width: min(900px, calc(100% - 24px)); max-height: calc(100% - 24px); box-sizing: border-box; padding: 16px; border: 1px solid #d5c8ac; border-radius: 8px; background: #fffaf0; color: #463e31; }
  dialog[open] { display: flex; flex-direction: column; gap: 12px; }
  dialog::backdrop { background: #0006; }
  h2 { margin: 0; font-size: 18px; } h3 { margin: 0 0 8px; font-size: 16px; }
  p { margin: 8px 0; font-size: 14px; } strong { font-size: 15px; font-weight: 500; }
  .content { min-height: 0; overflow: auto; container-type: inline-size; overflow-wrap: anywhere; }
  .body { white-space: pre-wrap; }
  .comparison { display: grid; grid-template-columns: minmax(0, 1fr); gap: 20px; }
  .comparison section { min-width: 0; }
  @container (min-width: 40rem) { .comparison { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .records { list-style: none; padding: 0; margin: 0; }
  .records li { margin: 8px 0; }
  .record { display: flex; flex-direction: column; align-items: stretch; gap: 6px; width: 100%; text-align: left; white-space: normal; overflow-wrap: anywhere; border-radius: 8px; padding: 12px; }
  .record span { font-size: 14px; }
  .excerpt { display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; white-space: pre-wrap; }
  footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  :global(html[data-theme="dark"]) dialog { background: #302d27; color: #f4e7cd; border-color: #6a5842; color-scheme: dark; }
</style>
