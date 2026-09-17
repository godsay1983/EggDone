<script lang="ts">
  import { onMount, tick } from "svelte";
  import PanelToolButton from "./PanelToolButton.svelte";
  import "./management-dialog.css";
  import { languageState, translator, type TranslationKey } from "$lib/i18n";
  import type { TrashItem } from "$lib/api/trashApi";
  import { createTrashStore } from "$lib/stores/trashStore";
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

  onMount(() => {
    // Disabling the focused restore button can move focus outside the dialog.
    const escape = (event: KeyboardEvent) => {
      if (dialog.open && event.key === "Escape") {
        event.preventDefault(); event.stopImmediatePropagation(); back();
      }
    };
    window.addEventListener("keydown", escape, true);
    dialog.showModal(); void load(true);
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
        try { await afterCommit(); refreshNeeded = false; message = "trash.restored"; }
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
  function back() { if (busy) return; if (pending) pending = null; else onClose(); }
</script>

<dialog class="management-dialog" bind:this={dialog} aria-labelledby="trash-heading" onkeydown={event => {
  event.stopPropagation();
  if (event.key === "Escape") { event.preventDefault(); back(); }
}}
  oncancel={event => { event.preventDefault(); back(); }}>
  <header><h2 id="trash-heading">{$translator(pending ? "trash.preview" : "trash.title")}</h2>
    {#if !pending}
      <div class="header-tools">
        <PanelToolButton icon="refresh" label={$translator("trash.refresh")} disabled={busy} onclick={() => load(true)} />
        <PanelToolButton icon="close" label={$translator("common.close")} disabled={busy} onclick={back} />
      </div>
    {/if}
  </header>
  <div class="content" bind:this={content} aria-busy={busy}>
    {#if message}<p role="status">{$translator(message)}</p>{/if}
    {#if busy}<p role="status">{$translator("common.loading")}</p>{/if}
    {#if loadFailed}<p role="alert">{$translator("trash.loadFailed")}</p>{/if}
    {#if pending}
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
          <li><button class="record" disabled={busy || loadFailed} onclick={() => select(item)}>
            <strong>{item.title || $translator("trash.untitled")}</strong>
            <span class="meta">{$translator(item.kind === "todo" ? "trash.todo" : "trash.note")} · {listDate(item.deleted_at)}</span>
            {#if item.content}<span class="excerpt">{item.content}</span>{/if}
          </button></li>
        {/each}
      </ul>
      {#if more}<button class="action-button" disabled={busy || loadFailed} onclick={() => load(false)}>{$translator("trash.more")}</button>{/if}
    {/if}
  </div>
  {#if pending}
    <footer>
      <button class="action-button" disabled={busy} onclick={back}>{$translator("common.cancel")}</button>
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
  .attachments { padding-left: 20px; overflow-wrap: anywhere; font-size: 14px; }
  footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  :global(html[data-theme="dark"]) dialog { background: #302d27; color: #f4e7cd; border-color: #6a5842; color-scheme: dark; }
</style>
