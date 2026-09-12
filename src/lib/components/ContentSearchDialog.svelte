<script lang="ts">
  import { onMount, tick } from "svelte";
  import { translator, type TranslationKey } from "$lib/i18n";
  import type { SearchItem, SearchTarget, SearchScope } from "$lib/api/contentSearchApi";
  import { createContentSearchStore, SEARCH_PAGE_SIZE } from "$lib/stores/contentSearchStore";
  export let active = true;
  export let onOpen: (item: SearchItem) => Promise<SearchTarget | null>;
  export let onClose: () => void;
  const search = createContentSearchStore();
  const labels: Record<SearchScope, TranslationKey> = { todo: "contentSearch.tasks", note: "contentSearch.notes", attachment: "contentSearch.files" };
  let dialog: HTMLDialogElement, content: HTMLDivElement, input: HTMLInputElement;
  let opening = false, disposed = false;
  let error: TranslationKey | null = null;
  let preview: SearchTarget | null = null;
  let scrollTop = 0, dialogScrollTop = 0;
  let category: SearchScope | "all" = "all";
  $: if (dialog) {
    if (active && !dialog.open) { dialog.showModal(); void restoreScroll(); }
    else if (!active && dialog.open) dialog.close();
  }
  onMount(() => {
    input.focus();
    const escape = (event: KeyboardEvent) => {
      if (active && event.key === "Escape") { event.preventDefault(); event.stopImmediatePropagation(); void back(); }
    };
    window.addEventListener("keydown", escape, true);
    return () => { disposed = true; search.dispose(); window.removeEventListener("keydown", escape, true); dialog.close(); };
  });
  async function restoreScroll() {
    await tick();
    if (!disposed && active && !preview) { content.scrollTop = scrollTop; dialog.scrollTop = dialogScrollTop; }
  }
  async function select(item: SearchItem) {
    if (opening) return;
    opening = true; error = null; scrollTop = content.scrollTop; dialogScrollTop = dialog.scrollTop;
    try {
      const target = await onOpen(item);
      if (!disposed) { preview = target; if (target) { await tick(); content.scrollTop = 0; dialog.scrollTop = 0; } }
    } catch (failure) {
      const code = failure instanceof Error ? failure.message : String(failure);
      error = code === "SEARCH_UNAVAILABLE" ? "contentSearch.unavailable" : "contentSearch.openFailed";
    } finally { opening = false; }
  }
  async function back() {
    if (opening) return;
    if (preview) { preview = null; await restoreScroll(); }
    else onClose();
  }
  function resetScroll() { scrollTop = 0; dialogScrollTop = 0; content.scrollTop = 0; dialog.scrollTop = 0; }
  function submit() { if (!opening) { error = null; resetScroll(); void search.search(); } }
  function change(value: string) { if (!opening) { error = null; search.setQuery(value); resetScroll(); } }
</script>

<dialog bind:this={dialog} aria-labelledby="content-search-heading" oncancel={event => { event.preventDefault(); void back(); }}>
  <header><h2 id="content-search-heading">{$translator("contentSearch.title")}</h2>
    <button class="action-button" disabled={opening} onclick={back}
      title={$translator(preview ? "contentSearch.back" : "common.close")}
      aria-label={$translator(preview ? "contentSearch.back" : "common.close")}>{preview ? "←" : "×"}</button>
  </header>
  {#if !preview}
    <form onsubmit={event => { event.preventDefault(); submit(); }}>
      <input bind:this={input} type="search" value={$search.query} disabled={opening}
        aria-label={$translator("contentSearch.placeholder")} placeholder={$translator("contentSearch.placeholder")}
        oninput={event => change(event.currentTarget.value)} />
      <button class="action-button" type="button" title={$translator("common.clear")} aria-label={$translator("common.clear")}
        disabled={opening || !$search.query} onclick={() => { change(""); input.focus(); }}>×</button>
      <button class="action-button" data-tone="primary" type="submit" disabled={opening}>{$translator("contentSearch.search")}</button>
    </form>
  {/if}
  <div class="content" bind:this={content}>
    {#if error}<p role="alert">{$translator(error)}</p>{/if}
    {#if opening}<p role="status">{$translator("common.loading")}</p>{/if}
    {#if preview}
      <p class="state">{$translator("contentSearch.archivedReadonly")}</p>
      <h3>{preview.title || $translator("trash.untitled")}</h3>
      <p>{$translator(preview.completed ? "contentSearch.completed" : "contentSearch.incomplete")}</p>
      <p class="body">{preview.content}</p>
    {:else if $search.invalid}
      <p role="alert">{$translator("contentSearch.invalid")}</p>
    {:else if $search.started}
      <select aria-label={$translator("contentSearch.category")} bind:value={category} disabled={opening}>
        <option value="all">{$translator("contentSearch.all")}</option>
        {#each $search.groups as group}
          <option value={group.scope}>{$translator(labels[group.scope])}{group.status === "ready" ? ` (${group.total})` : ""}</option>
        {/each}
      </select>
      {#each $search.groups as group (group.scope)}
        {#if category === "all" || category === group.scope}
        <section class="group" data-scope={group.scope} aria-busy={group.status === "loading"}>
          <h3>{$translator(labels[group.scope])}{#if group.status === "ready"}<span> {group.total}</span>{/if}</h3>
          {#if group.status === "loading"}<p role="status">{$translator("common.loading")}</p>
          {:else if group.status === "failed"}
            <p role="alert">{$translator("contentSearch.failed")}</p>
            <button class="action-button" disabled={opening} onclick={() => search.page(group.scope, group.offset)}>{$translator("common.retry")}</button>
          {:else if group.status === "ready"}
            {#if !group.items.length}<p>{$translator("contentSearch.empty")}</p>{/if}
            <ul>
              {#each group.items as item (item.uuid)}
                <li><button class="result action-button" disabled={opening} onclick={() => select(item)}>
                  <strong>{item.title || $translator("trash.untitled")}</strong>
                  {#if item.kind === "todo"}<span class="state">{$translator(item.archived ? "contentSearch.archived" : item.completed ? "contentSearch.completed" : "contentSearch.incomplete")}</span>{/if}
                  {#if item.parent_uuid}<span>{$translator("contentSearch.inNote")}: {item.parent_title || $translator("trash.untitled")}</span>{/if}
                  <span class="excerpt">{item.excerpt}</span>
                </button></li>
              {/each}
            </ul>
            {#if group.offset > 0 || group.total > SEARCH_PAGE_SIZE}
              <div class="pages">
                <button class="action-button" disabled={opening || group.offset === 0} onclick={() => search.page(group.scope, Math.max(0, group.offset - SEARCH_PAGE_SIZE))}>{$translator("contentSearch.previous")}</button>
                <span>{$translator("contentSearch.page", { page: Math.floor(group.offset / SEARCH_PAGE_SIZE) + 1 })}</span>
                <button class="action-button" disabled={opening || group.offset + SEARCH_PAGE_SIZE >= group.total || group.offset + SEARCH_PAGE_SIZE > 100000}
                  onclick={() => search.page(group.scope, group.offset + SEARCH_PAGE_SIZE)}>{$translator("contentSearch.next")}</button>
              </div>
            {/if}
          {/if}
        </section>
        {/if}
      {/each}
    {/if}
  </div>
</dialog>

<style>
  dialog { width: min(900px, calc(100% - 24px)); height: min(740px, calc(100% - 24px)); max-height: calc(100% - 24px); box-sizing: border-box; padding: 16px; border: 1px solid #d5c8ac; border-radius: 8px; background: #fffaf0; color: #463e31; }
  dialog[open] { display: flex; flex-direction: column; gap: 12px; } dialog::backdrop { background: #0006; }
  h2 { font-size: 18px; margin: 0; } h3 { font-size: 16px; margin: 0 0 8px; overflow-wrap: anywhere; }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; flex-shrink: 0; }
  header h2 { min-width: 0; overflow-wrap: anywhere; } header button { flex-shrink: 0; }
  h3 span, .state { font-size: 14px; font-weight: normal; }
  h3 span { margin-left: 6px; }
  form { display: flex; flex-wrap: wrap; gap: 8px; } input { flex: 1 1 160px; min-width: 0; width: 100%; box-sizing: border-box; font: inherit; padding: 8px 12px; border: 1px solid #86765c; border-radius: 8px; color: inherit; background: transparent; }
  .content { min-height: 0; flex: 1; overflow: auto; overflow-wrap: anywhere; } .body { white-space: pre-wrap; }
  select { max-width: 100%; padding: 8px; font: inherit; color: inherit; background: inherit; border: 1px solid #86765c; border-radius: 8px; }
  .group { padding: 12px 0; border-bottom: 1px solid #86765c; } .group:last-child { border: 0; }
  ul { list-style: none; margin: 0; padding: 0; } li { margin: 8px 0; }
  .result { width: 100%; display: flex; flex-direction: column; align-items: stretch; text-align: left; gap: 4px; white-space: normal; overflow-wrap: anywhere; padding: 12px; border-radius: 8px; }
  .result strong { font-size: 15px; font-weight: 500; } .result span { font-size: 14px; }
  .excerpt { white-space: pre-wrap; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; }
  .pages { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  @media (max-height: 500px) {
    dialog[open] { display: block; overflow: auto; }
    header { position: sticky; top: -16px; background: inherit; margin: -16px -16px 12px; padding: 16px; z-index: 1; }
    form { margin-bottom: 12px; } .content { overflow: visible; }
  }
  :global(html[data-theme="dark"]) dialog { background: #302d27; color: #f4e7cd; border-color: #6a5842; color-scheme: dark; }
</style>
