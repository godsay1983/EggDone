<script lang="ts">
  import { onMount } from 'svelte';
  import { translator, type TranslationKey } from '$lib/i18n';
  import { spaceActivationApi, type SpaceReport } from '$lib/api/spaceActivationApi';
  export let onBusy: (value: boolean) => void = () => {};
  export let onActivated: () => Promise<void> = async () => {};
  let report: SpaceReport | null = null;
  let busy = false;
  let agreed = false;
  let message: TranslationKey | null = null;
  async function run(action: 'status' | 'prepare' | 'activate') {
    if (busy || (action === 'activate' && (!agreed || !report?.confirmation))) return;
    busy = true; onBusy(true); message = null;
    const expected = action === 'activate' ? report?.confirmation ?? null : null;
    if (action !== 'activate') agreed = false;
    try {
      report = await spaceActivationApi(action, expected);
      if (report.state === 'active' && action === 'activate') await onActivated();
    } catch (error) {
      const code = error instanceof Error ? error.message : String(error);
      message = code.includes('LOCAL_NOT_SETTLED') ? 'migrationBackup.unsettled' :
        code.includes('BUSY') ? 'migrationBackup.busy' : code.includes('LIMIT') ? 'migrationBackup.limit' :
        code.includes('CHANGED') ? 'migrationBackup.changed' : 'space.failed';
    } finally { busy = false; onBusy(false); }
  }
  onMount(() => { void run('status'); });
</script>
<section aria-label={$translator('migrationBackup.title')} aria-busy={busy}>
  {#if report?.state === 'active'}
    <p role="status">{$translator('space.active')}</p>
  {:else}
    <p>{$translator('space.scope')}</p>
    {#if report?.state === 'prepared'}
      <p role="status">{$translator(report.mode === 'join' ? 'space.joinReady' : 'space.ready')}</p>
      <label><input type="checkbox" bind:checked={agreed} disabled={busy} /><span>{$translator('space.agree')}</span></label>
    {/if}
    <div class="actions">
      <button class="action-button" disabled={busy} onclick={() => run('prepare')}>{$translator('space.prepare')}</button>
      {#if report?.state === 'prepared'}
        <button class="action-button" data-tone="primary" disabled={busy || !agreed} onclick={() => run('activate')}>{$translator(report.mode === 'join' ? 'space.join' : 'space.activate')}</button>
      {/if}
    </div>
  {/if}
  {#if busy}<p role="status">{$translator('space.working')}</p>{/if}
  {#if message}<p role="alert">{$translator(message)}</p>{/if}
</section>
<style>
  section { padding: 0; }
  p { margin: 8px 0; font-size: .875rem; overflow-wrap: anywhere; }
  label { display: flex; align-items: flex-start; gap: 8px; margin: 12px 0; font-size: .875rem; }
  input { flex: 0 0 auto; margin-top: 3px; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; margin: 12px 0; }
  .actions button { min-height: 36px; font-size: .875rem; padding: 6px 12px; }
</style>
