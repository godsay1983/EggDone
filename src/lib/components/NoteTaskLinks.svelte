<script lang="ts">
  import { translator } from "$lib/i18n";
  import { createTaskNoteLinkStore } from "$lib/stores/taskNoteLinkStore";
  export let uuid: string;
  export let revision: unknown;
  const links = createTaskNoteLinkStore();
  let expanded = false;
  $: { revision; void links.load(uuid); }
</script>

{#if $links.failed || $links.loading || $links.items.length > 0}
  <section class="note-task-links" aria-label={$translator("links.tasks")}>
    {#if $links.loading}<small role="status">{$translator("common.loading")}</small>{/if}
    {#if $links.failed}
      <p role="alert">{$translator("links.loadFailed")}</p>
      <button class="action-button" onclick={() => void links.load(uuid)}>{$translator("common.retry")}</button>
    {/if}
    {#if $links.items.length}
      <button class="action-button" aria-expanded={expanded} onclick={() => expanded = !expanded}>
        {$translator("links.tasks")} ({$links.items.length})
      </button>
      {#if expanded}
        <ul>
          {#each $links.items as item (item.link.uuid)}
            <li><span title={item.todo_title ?? ""}>{item.todo_title ?? $translator("links.unavailable")}</span>
              <small>{$translator(`links.state.${item.todo_state}`)}{item.is_repeating ? " · " + $translator("links.thisOnly") : ""}</small></li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>
{/if}

<style>
  .note-task-links { padding: 8px 12px; flex: 0 1 auto; min-height: 0; max-height: 180px; overflow: auto; }
  ul { margin: 8px 0 0; padding: 0; list-style: none; }
  li { display: flex; flex-wrap: wrap; gap: 4px 12px; padding: 6px 0; border-bottom: 1px solid var(--note-border); }
  li span { flex: 1 1 150px; overflow-wrap: anywhere; }
  small { font-size: 12px; line-height: 1.5; }
  p { margin: 4px 0; font-size: 13px; }
</style>
