<script lang="ts">
  import { translator } from '$lib/i18n';
  import { dailyPlans } from '$lib/stores/dailyPlanStore';
  export let showLoading = false;
</script>

{#if $dailyPlans.error}
  <div class="plan-status" role="status">
    <span>{$translator(`dailyPlan.error.${$dailyPlans.error}`)}</span>
    <div class="status-actions">
      {#if $dailyPlans.pending || $dailyPlans.error !== 'conflict' && $dailyPlans.error !== 'rollover' && $dailyPlans.error !== 'unavailable'}
        <button class="action-button" disabled={$dailyPlans.writing || $dailyPlans.loading}
          onclick={() => void dailyPlans.retry()}>{$translator($dailyPlans.pending ? 'dailyPlan.retry' : 'common.retry')}</button>
      {/if}
      {#if $dailyPlans.pending || $dailyPlans.error === 'conflict' || $dailyPlans.error === 'rollover' || $dailyPlans.error === 'unavailable'}
        <button class="action-button" disabled={$dailyPlans.writing || $dailyPlans.loading}
          onclick={() => void dailyPlans.dismiss()}>{$translator('dailyPlan.dismiss')}</button>
      {/if}
    </div>
  </div>
{:else if showLoading && ($dailyPlans.loading || $dailyPlans.writing)}
  <p class="plan-status" role="status">{$translator($dailyPlans.writing ? 'dailyPlan.saving' : 'common.loading')}</p>
{/if}

<style>
  .plan-status { margin: 6px 0; font-size: 12px; overflow-wrap: anywhere; flex-shrink: 0; }
  .status-actions { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
</style>
