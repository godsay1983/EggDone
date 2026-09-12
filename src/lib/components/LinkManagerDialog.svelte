<script lang="ts">
  import { onMount } from "svelte";
  import { translator } from "$lib/i18n";
  import type { LinkScope, TaskNoteLinkView } from "$lib/types/taskNoteLink";
  import { createLinkManager, type LinkCandidate, type LinkChange } from "$lib/stores/linkManagerStore";
  export let scope: LinkScope;
  export let uuid: string;
  export let title: string;
  export let saveSource: () => Promise<void>;
  export let afterCommit: () => Promise<void>;
  export let onClose: () => void;
  const manager = createLinkManager();
  let dialog: HTMLDialogElement;
  let links: TaskNoteLinkView[] = [];
  let candidates: LinkCandidate[] = [];
  let query = "";
  let busy = false;
  let loading = false;
  let loadFailed = false;
  let message = "";
  let pending: LinkChange | null = null;
  let pendingTitle = "";
  $: filtered = candidates.filter(item => item.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
  $: limitReached = scope === "todo" && links.length >= 20;
  onMount(() => { dialog.showModal(); void load(); return () => dialog.close(); });
  async function load() {
    loading = true; loadFailed = false;
    try { const data = await manager.load(scope, uuid); links = data.links; candidates = data.candidates; }
    catch { loadFailed = true; } finally { loading = false; }
  }
  async function select(candidate: LinkCandidate) {
    if (busy || loading || loadFailed) return;
    busy = true; message = "";
    try { pending = await manager.prepare(scope, uuid, candidate.uuid); pendingTitle = candidate.title; }
    catch { message = $translator("links.changeFailed"); } finally { busy = false; }
  }
  function unlink(item: TaskNoteLinkView) {
    pending = { todoUuid: item.link.todo_uuid, noteUuid: item.link.note_uuid, active: false, expected: structuredClone(item.link) };
    pendingTitle = (scope === "todo" ? item.note_title : item.todo_title) ?? $translator("links.unavailable");
    message = "";
  }
  async function confirm() {
    if (!pending || busy) return;
    busy = true; message = "";
    try {
      const failed = await manager.change(pending, saveSource, async () => { pending = null; await afterCommit(); });
      message = $translator(failed ? "links.changeRefreshFailed" : "links.changed");
    } catch { pending = null; message = $translator("links.changeFailed"); }
    finally { await load(); busy = false; }
  }
</script>

<dialog bind:this={dialog} onkeydown={event => event.stopPropagation()} oncancel={event => {
  event.preventDefault(); if (!busy) { if (pending) pending = null; else onClose(); }
}}>
  <header><h2>{$translator(scope === "todo" ? "links.notes" : "links.tasks")}</h2><p>{title}</p></header>
  <div class="content">
    {#if message}<p role="status">{message}</p>{/if}
    {#if loading}<p role="status">{$translator("common.loading")}</p>{/if}
    {#if loadFailed}<p role="alert">{$translator("links.managerLoadFailed")}</p><button class="action-button" disabled={busy || loading} onclick={load}>{$translator("common.retry")}</button>{/if}
    {#if pending}
      <h3>{$translator(pending.active ? "links.confirmLink" : "links.confirmUnlink")}</h3>
      <p>{pendingTitle || $translator("links.untitled")}</p>
      <p>{$translator(pending.active ? "links.linkHint" : "links.unlinkHint")}</p>
    {:else}
      <ul>
        {#each links as item (item.link.uuid)}
          <li><div><span>{(scope === "todo" ? item.note_title : item.todo_title) ?? $translator("links.unavailable")}</span>
            <small>{$translator(scope === "todo" && item.note_state === "active" ? "links.available" : `links.state.${scope === "todo" ? item.note_state : item.todo_state}`)}{item.is_repeating ? " · " + $translator("links.thisOnly") : ""}</small></div>
            <button class="action-button" disabled={busy || loading || loadFailed} onclick={() => unlink(item)}>{$translator("links.unlink")}</button></li>
        {/each}
      </ul>
      <h3>{$translator("links.addExisting")}</h3>
      <input type="search" bind:value={query} aria-label={$translator("links.search")} placeholder={$translator("links.search")} disabled={busy} />
      {#if limitReached}<p>{$translator("links.limit")}</p>{/if}
      {#if !loading && !loadFailed && !filtered.length}<p>{$translator("links.noCandidates")}</p>{/if}
      <ul>{#each filtered as item (item.uuid)}<li><button class="candidate action-button" disabled={busy || loading || loadFailed || limitReached} onclick={() => select(item)}>{item.title || $translator("links.untitled")}</button></li>{/each}</ul>
    {/if}
  </div>
  <footer>
    {#if pending}
      <button class="action-button" disabled={busy} onclick={() => pending = null}>{$translator("common.cancel")}</button>
      <button class="action-button" data-tone={pending.active ? "primary" : "danger"} disabled={busy} onclick={confirm}>{$translator("common.confirm")}</button>
    {:else}<button class="action-button" disabled={busy} onclick={onClose}>{$translator("common.close")}</button>{/if}
  </footer>
</dialog>

<style>
  dialog { width: min(560px, calc(100% - 24px)); max-height: calc(100dvh - 24px); padding: 16px; border: 1px solid #d5c8ac; border-radius: 8px; background: #fffaf0; color: #463e31; }
  dialog[open] { display: flex; flex-direction: column; gap: 12px; }
  dialog::backdrop { background: #0006; }
  h2 { margin: 0; font-size: 18px; } h3 { font-size: 15px; margin: 12px 0 8px; }
  p { margin: 6px 0; font-size: 14px; overflow-wrap: anywhere; }
  header p { max-height: 48px; overflow: auto; }
  .content { min-height: 0; overflow: auto; }
  ul { margin: 0; padding: 0; list-style: none; }
  li { display: flex; align-items: center; gap: 8px; padding: 6px 0; flex-wrap: wrap; }
  li div { flex: 1 1 160px; min-width: 0; overflow-wrap: anywhere; }
  small { display: block; font-size: 12px; margin-top: 4px; }
  input { width: 100%; box-sizing: border-box; padding: 8px; border-radius: 6px; border: 1px solid #9e8e74; font: inherit; color: inherit; background: transparent; }
  .candidate { width: 100%; text-align: left; white-space: normal; overflow-wrap: anywhere; }
  footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
  :global(html[data-theme="dark"]) dialog { border-color: #6a5842; background: #302d27; color: #f4e7cd; color-scheme: dark; }
</style>
