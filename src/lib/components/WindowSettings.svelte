<script lang="ts">
  import { translator } from "$lib/i18n";
  import { windowPreferences, windowPreferenceError, windowPreferenceBusy, updateWindowPreferences } from "$lib/stores/windowPreferences";
  import { WINDOW_PRESETS, ZOOM_LEVELS } from "$lib/utils/windowPreferences";
  const presets = ["small", "comfortable", "large"] as const;
</script>

<section class="language-settings-section" aria-labelledby="window-settings-title">
  <div class="language-settings-heading"><strong id="window-settings-title">{$translator("window.title")}</strong></div>
  <div class="language-options" role="group" aria-label={$translator("window.size")}>
    {#each presets as preset}
      <button type="button" disabled={$windowPreferenceBusy} onclick={() => void updateWindowPreferences(WINDOW_PRESETS[preset])}>{$translator(`window.${preset}`)}</button>
    {/each}
  </div>
  <label class="shortcut-select">
    <span>{$translator("window.zoom")}</span>
    <select value={$windowPreferences.zoom} disabled={$windowPreferenceBusy} onchange={(event) => void updateWindowPreferences({ zoom: Number(event.currentTarget.value) })}>
      {#each ZOOM_LEVELS as zoom}<option value={zoom}>{Math.round(zoom * 100)}%</option>{/each}
    </select>
  </label>
  <div class="language-options"><button type="button" disabled={$windowPreferenceBusy} onclick={() => void updateWindowPreferences(WINDOW_PRESETS.small)}>{$translator("window.reset")}</button></div>
  {#if $windowPreferenceError}<p role="alert">{$translator("window.error")}</p>{/if}
</section>
