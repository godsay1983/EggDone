<script lang="ts">
  import { translator } from "$lib/i18n";
  export let active = false;
  export let busy = false;
  export let onBack: () => Promise<boolean>;
  function modal(node: HTMLDialogElement) {
    node.showModal();
    return { destroy: () => node.close() };
  }
</script>

{#if active}
  <dialog use:modal aria-label={$translator("links.content")} aria-busy={busy}
    onkeydown={event => event.stopPropagation()}
    oncancel={event => { event.preventDefault(); if (!busy) void onBack(); }}>
    <slot />
  </dialog>
{:else}<slot />{/if}

<style>
  dialog { width: min(960px, calc(100% - 24px)); height: min(900px, calc(100dvh - 24px)); max-height: calc(100dvh - 24px); box-sizing: border-box; padding: 12px; border: 1px solid #d5c8ac; border-radius: 8px; background: #fffaf0; color: #463e31; }
  dialog[open] { display: flex; flex-direction: column; gap: 10px; overflow: auto; }
  dialog::backdrop { background: #0006; }
  :global(html[data-theme="dark"]) dialog { border-color: #6a5842; background: #302d27; color: #f4e7cd; }
</style>
