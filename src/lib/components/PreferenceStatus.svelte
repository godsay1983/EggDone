<script lang="ts">
  import { preferenceStorageFailures } from '$lib/utils/preferenceStorage';
  import { translator } from '$lib/i18n';
</script>

{#if $preferenceStorageFailures.length > 0}
  <div role="alert" class="preference-status">
    {#if $preferenceStorageFailures.includes('read')}<p>{$translator('settings.preferenceReadFailed')}</p>{/if}
    {#if $preferenceStorageFailures.includes('write')}<p>{$translator('settings.preferenceWriteFailed')}</p>{/if}
  </div>
{/if}

<style>
  .preference-status { flex: 0 0 auto; padding: 8px; overflow-wrap: anywhere; }
  p { margin: 0; font-size: 12px; line-height: 1.5; }
</style>
