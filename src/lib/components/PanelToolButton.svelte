<script lang="ts">
  export let icon: 'search' | 'refresh' | 'close';
  export let label: string;
  export let disabled = false;
  export let onclick: () => void = () => {};
  export let type: 'button' | 'submit' = 'button';
</script>

<button {type} {disabled} {onclick} aria-label={label} title={label}>
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor"
    stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
    {#if icon === 'search'}<circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 4 4" />
    {:else if icon === 'refresh'}<path d="M20 7v5h-5M20 12a8 8 0 1 0-2.4 5.7" />
    {:else}<path d="m6 6 12 12M6 18 18 6" />{/if}
  </svg>
</button>

<style>
  button { display: inline-flex; align-items: center; justify-content: center; flex: none;
    width: 36px; height: 36px; padding: 0; border: 0; border-radius: 6px; background: transparent; color: inherit; cursor: pointer; }
  button:hover:not(:disabled) { background: #9c8a6320; }
  button:focus-visible { outline: 2px solid #b28b20; outline-offset: -2px; }
  button:disabled { opacity: .5; cursor: default; }
</style>
