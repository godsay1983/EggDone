<script lang="ts">
  import { onMount } from 'svelte';
  import { translator } from '$lib/i18n';
  import MigrationPreparation from './MigrationPreparation.svelte';
  import PanelToolButton from './PanelToolButton.svelte';
  import './management-dialog.css';
  export let onClose: () => void;
  export let onActivated: () => Promise<void>;
  let dialog: HTMLDialogElement;
  let busy = false;
  function close() { if (!busy) onClose(); }
  onMount(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault(); event.stopImmediatePropagation(); close();
      }
    };
    window.addEventListener('keydown', escape, true);
    dialog.showModal();
    return () => { window.removeEventListener('keydown', escape, true); dialog.close(); };
  });
</script>

<dialog class="management-dialog" bind:this={dialog} aria-labelledby="sync-space-heading"
  oncancel={event => { event.preventDefault(); close(); }}>
  <header>
    <h2 id="sync-space-heading">{$translator('sync.space')}</h2>
    <PanelToolButton icon="close" label={$translator('common.close')} disabled={busy} onclick={close} />
  </header>
  <div class="content">
    <MigrationPreparation onBusy={value => busy = value} {onActivated} />
  </div>
</dialog>

<style>
  dialog { width: min(560px, calc(100% - 24px)); max-height: calc(100% - 24px); box-sizing: border-box; border: 1px solid var(--action-border); border-radius: 8px; background: var(--panel-bg, #fffaf0); color: var(--action-text); }
  dialog[open] { display: flex; flex-direction: column; }
  dialog::backdrop { background: #0006; }
  h2 { margin: 0; font-size: 18px; }
  .content { min-height: 0; overflow: auto; }
  :global(html[data-theme="dark"]) dialog { background: #302d27; color: #f4e7cd; }
</style>
