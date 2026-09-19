<script lang="ts">
  import { translator } from '$lib/i18n';
  export let value: 'due' | 'planned' = 'due';
  export let onChange: (value: 'due' | 'planned') => void;
</script>

<div class="plan-tabs" role="group" aria-label={$translator('dailyPlan.title')}>
  <button class="action-button" aria-pressed={value === 'planned'} data-tone={value === 'planned' ? 'primary' : undefined}
    onclick={() => onChange('planned')}>{$translator('dailyPlan.planned')}</button>
  <button class="action-button" aria-pressed={value === 'due'} data-tone={value === 'due' ? 'primary' : undefined}
    onclick={() => onChange('due')}>{$translator('dailyPlan.due')}</button>
</div>

<style>
  .plan-tabs { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; margin: 0 0 8px; flex-shrink: 0; }
  button { min-width: 0; }
</style>
