<script lang="ts">
  import { isTauri, invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { translator } from "$lib/i18n";
  import { windowPreferenceError } from "$lib/stores/windowPreferences";
  const directions = ["North", "South", "East", "West", "NorthEast", "NorthWest", "SouthEast", "SouthWest"] as const;
  async function resize(event: PointerEvent, direction: typeof directions[number]) {
    if (event.button !== 0 || !isTauri()) return;
    event.preventDefault();
    try {
      await invoke("mark_panel_interaction");
      await getCurrentWindow().startResizeDragging(direction);
    } catch { windowPreferenceError.set(true); }
  }
</script>

{#each directions as direction}
  <button class="window-resize" class:corner={direction.length > 5}
    data-direction={direction} tabindex="-1" aria-label={$translator("window.resize")}
    title={$translator("window.resize")} onpointerdown={(event) => void resize(event, direction)}></button>
{/each}

<style>
  .window-resize { position: fixed; z-index: 10000; padding: 0; margin: 0; border: 0; border-radius: 0; background: transparent; touch-action: none; }
  [data-direction="North"], [data-direction="South"] { left: 10px; right: 10px; height: 5px; cursor: ns-resize; }
  [data-direction="East"], [data-direction="West"] { top: 10px; bottom: 10px; width: 5px; cursor: ew-resize; }
  [data-direction^="North"] { top: 0; }
  [data-direction^="South"] { bottom: 0; }
  [data-direction$="East"] { right: 0; }
  [data-direction$="West"] { left: 0; }
  .corner { width: 10px; height: 10px; }
  [data-direction="NorthEast"], [data-direction="SouthWest"] { cursor: nesw-resize; }
  [data-direction="NorthWest"], [data-direction="SouthEast"] { cursor: nwse-resize; }
  [data-direction="SouthEast"]::after { content: ""; position: absolute; inset: 2px; border-right: 1px solid currentColor; border-bottom: 1px solid currentColor; opacity: 0.5; }
</style>
