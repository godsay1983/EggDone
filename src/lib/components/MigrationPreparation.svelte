<script lang="ts">
  import { onMount } from 'svelte';
  import { translator, type TranslationKey } from '$lib/i18n';
  import { migrationBackupApi, type MigrationBackupReport } from '$lib/api/migrationBackupApi';
  export let onBusy: (value: boolean) => void = () => {};
  let report: MigrationBackupReport | null = null;
  let busy = false;
  let message: TranslationKey | null = null;
  let verifiedNow = false;
  async function run(action: 'status' | 'prepare' | 'verify') {
    if (busy) return;
    busy = true; onBusy(true); message = null; verifiedNow = false;
    try { report = await migrationBackupApi(action); verifiedNow = action !== 'status' && !!report?.current; }
    catch (error) {
      const code = error instanceof Error ? error.message : String(error);
      message = code.includes('ASSET') ? 'migrationBackup.assetMissing' :
        code.includes('CHANGED') ? 'migrationBackup.changed' : code.includes('BUSY') ? 'migrationBackup.busy' :
        code.includes('LIMIT') ? 'migrationBackup.limit' : 'migrationBackup.failed';
    } finally { busy = false; onBusy(false); }
  }
  onMount(() => { void run('status'); });
</script>

<section aria-label={$translator('migrationBackup.title')} aria-busy={busy}>
  <p>{$translator('migrationBackup.scope')}</p>
  {#if report}
    <p>{$translator('migrationBackup.count', { count: report.files, size: (report.bytes / 1048576).toFixed(1) })}</p>
    <p role="status">{$translator(!report.current ? 'migrationBackup.changed' : verifiedNow ? 'migrationBackup.verified' : 'migrationBackup.saved')}</p>
    {#if report.blockers.length}<p>{$translator('migrationBackup.unsettled')}</p>{/if}
  {/if}
  {#if message}<p role="alert">{$translator(message)}</p>{/if}
  {#if busy}<p role="status">{$translator('migrationBackup.working')}</p>{/if}
  <div class="actions">
    <button class="action-button" disabled={busy} onclick={() => run('prepare')}>{$translator('migrationBackup.prepare')}</button>
    {#if report}<button class="action-button" disabled={busy} onclick={() => run('verify')}>{$translator('migrationBackup.verify')}</button>{/if}
  </div>
  <p class="boundary">{$translator('migrationBackup.boundary')}</p>
</section>
<style>
  section { padding: 0; }
  p { margin: 6px 0; font-size: .875rem; overflow-wrap: anywhere; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; margin: 12px 0; }
  .actions button { font-size: .875rem; padding: 6px 12px; min-height: 36px; }
  .boundary { opacity: .85; }
</style>
