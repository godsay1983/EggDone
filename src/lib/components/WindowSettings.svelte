<script lang="ts">
  import { translator } from "$lib/i18n";
  import { windowPreferences, windowPreferenceError, windowPreferenceBusy, updateWindowPreferences } from "$lib/stores/windowPreferences";
  import { WINDOW_PRESETS, ZOOM_LEVELS } from "$lib/utils/windowPreferences";
  const presets = ["small", "comfortable", "large"] as const;
</script>

<section class="language-settings-section window-settings" aria-labelledby="window-settings-title">
  <div class="language-settings-heading"><strong id="window-settings-title">{$translator("window.title")}</strong></div>
  <div class="language-options" role="group" aria-label={$translator("window.size")}>
    {#each presets as preset}
      <button type="button" disabled={$windowPreferenceBusy} onclick={() => void updateWindowPreferences(WINDOW_PRESETS[preset])}>{$translator(`window.${preset}`)}</button>
    {/each}
  </div>
  <label class="shortcut-select window-zoom">
    <span>{$translator("window.zoom")}</span>
    <select value={$windowPreferences.zoom} disabled={$windowPreferenceBusy} onchange={(event) => void updateWindowPreferences({ zoom: Number(event.currentTarget.value) })}>
      {#each ZOOM_LEVELS as zoom}<option value={zoom}>{Math.round(zoom * 100)}%</option>{/each}
    </select>
  </label>
  <div class="language-options"><button type="button" disabled={$windowPreferenceBusy} onclick={() => void updateWindowPreferences(WINDOW_PRESETS.small)}>{$translator("window.reset")}</button></div>
  {#if $windowPreferenceError}<p role="alert">{$translator("window.error")}</p>{/if}
</section>

<style>
  .window-settings {
    display: grid;
    gap: 12px;
  }

  .window-settings :global(.language-options) {
    margin-top: 0;
  }

  .window-zoom {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 96px;
    align-items: center;
    gap: 12px;
    padding: 0;
  }

  .window-zoom > span {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .window-zoom select {
    width: 100%;
    min-height: 32px;
    border-radius: 999px;
    padding: 6px 12px;
    font-size: 11px;
  }
</style>
