<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { translator } from "$lib/i18n";
  import {
    clearFocusTarget,
    FOCUS_SETTINGS_CHANGED_EVENT,
    FOCUS_TARGET_CHANGED_EVENT,
    getFocusDurations,
    getFocusTarget,
    type FocusTarget,
    type FocusDurations,
    type FocusPhase,
  } from "$lib/utils/focusSettings";

  let focusPhase: FocusPhase = "focus";
  let focusDurations: FocusDurations = getFocusDurations();
  let focusRunning = false;
  let focusEndsAt: number | null = null;
  let focusRemainingMs = focusDurations.focus;
  let focusDisplayTime = "25:00";
  let focusDisplayHint = "";
  let focusDisplayPhase = "";
  let focusIllustrationSrc = "/focus-illustration.png";
  let focusTarget: FocusTarget | null = getFocusTarget();
  let completingTarget = false;
  let focusCompletionVisible = false;
  let focusCompletionTimer: number | null = null;
  let compactMode = false;

  $: focusDisplayPhase = focusCompletionVisible
    ? $translator("focus.completed")
    : focusPhase === "focus"
      ? $translator("focus.title")
      : $translator("focus.break");
  $: focusDisplayTime = formatFocusTime(focusRemainingMs);
  $: focusDisplayHint = focusCompletionVisible
    ? $translator("focus.hintCompleted")
    : focusRunning
      ? $translator("focus.hintRunning")
      : focusRemainingMs === focusDurations[focusPhase]
        ? $translator("focus.hintReady")
        : $translator("focus.hintPaused");
  $: focusIllustrationSrc = focusCompletionVisible
    ? "/focus-done.png"
    : focusPhase === "break"
      ? "/focus-break.png"
      : "/focus-illustration.png";

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    refreshThemeFromStorage();
    refreshFocusDurations();
    refreshFocusTarget();
    const focusInterval = window.setInterval(updateFocusTimer, 1000);
    window.addEventListener(FOCUS_SETTINGS_CHANGED_EVENT, refreshFocusDurations);
    window.addEventListener(FOCUS_TARGET_CHANGED_EVENT, refreshFocusTarget);
    window.addEventListener("storage", refreshFocusFromStorage);
    window.addEventListener("focus", refreshThemeFromStorage);
    window.addEventListener("focus", refreshFocusDurations);
    window.addEventListener("focus", refreshFocusTarget);
    document.addEventListener("visibilitychange", refreshThemeFromStorage);
    document.addEventListener("visibilitychange", refreshFocusDurations);
    document.addEventListener("visibilitychange", refreshFocusTarget);
    if (isTauri()) {
      void listen("focus-start", () => {
        startFocusSession("focus");
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("focus-toggle", () => {
        toggleFocusFromTray();
      }).then((unlisten) => unlisteners.push(unlisten));
      void listen("focus-end", () => {
        void endFocusSession();
      }).then((unlisten) => unlisteners.push(unlisten));
    }
    return () => {
      unlisteners.forEach((unlisten) => unlisten());
      clearFocusCompletionTimer();
      window.clearInterval(focusInterval);
      window.removeEventListener(
        FOCUS_SETTINGS_CHANGED_EVENT,
        refreshFocusDurations,
      );
      window.removeEventListener(FOCUS_TARGET_CHANGED_EVENT, refreshFocusTarget);
      window.removeEventListener("storage", refreshFocusFromStorage);
      window.removeEventListener("focus", refreshThemeFromStorage);
      window.removeEventListener("focus", refreshFocusDurations);
      window.removeEventListener("focus", refreshFocusTarget);
      document.removeEventListener("visibilitychange", refreshThemeFromStorage);
      document.removeEventListener("visibilitychange", refreshFocusDurations);
      document.removeEventListener("visibilitychange", refreshFocusTarget);
    };
  });

  function refreshThemeFromStorage() {
    const savedTheme = localStorage.getItem("eggdone-theme");
    const theme =
      savedTheme === "light" || savedTheme === "dark"
        ? savedTheme
        : window.matchMedia("(prefers-color-scheme: dark)").matches
          ? "dark"
          : "light";
    document.documentElement.dataset.theme = theme;
  }

  function updateFocusTrayTooltip() {
    if (!isTauri()) return;
    void invoke("update_focus_tray_tooltip", {
      phase: focusPhase,
      remainingMs: Math.max(0, Math.ceil(focusRemainingMs)),
      title: focusTarget?.title ?? null,
    }).catch(() => {});
  }

  function restoreTrayTaskTooltip() {
    if (!isTauri()) return;
    void invoke("restore_tray_task_tooltip").catch(() => {});
  }

  function refreshFocusDurations() {
    const previous = focusDurations;
    const next = getFocusDurations();
    focusDurations = next;
    if (
      !focusRunning &&
      focusEndsAt === null &&
      focusRemainingMs === previous[focusPhase]
    ) {
      focusRemainingMs = next[focusPhase];
    }
  }

  function refreshFocusTarget() {
    focusTarget = getFocusTarget();
  }

  function refreshFocusFromStorage(event: StorageEvent) {
    if (!event.key || event.key === "eggdone-theme") {
      refreshThemeFromStorage();
    }
    if (!event.key || event.key.startsWith("eggdone-focus-")) {
      refreshFocusDurations();
      refreshFocusTarget();
    }
  }

  function startFocusSession(phase: FocusPhase = "focus") {
    clearFocusCompletionTimer();
    focusCompletionVisible = false;
    refreshFocusDurations();
    focusPhase = phase;
    focusRemainingMs = focusDurations[phase];
    focusEndsAt = Date.now() + focusRemainingMs;
    focusRunning = true;
    updateFocusTrayTooltip();
  }

  function updateFocusTimer() {
    if (!focusRunning || focusEndsAt === null) return;
    focusRemainingMs = Math.max(0, focusEndsAt - Date.now());
    updateFocusTrayTooltip();
    if (focusRemainingMs > 0) return;
    const completedPhase = focusPhase;
    focusRunning = false;
    focusEndsAt = null;
    focusRemainingMs = 0;
    focusCompletionVisible = true;
    updateFocusTrayTooltip();
    if (isTauri()) {
      void invoke("publish_focus_notification", { completedPhase }).catch(() => {});
    }
    scheduleNextFocusPhase(completedPhase);
  }

  function toggleFocusRunning() {
    if (focusCompletionVisible) return;
    if (focusRunning) {
      updateFocusTimer();
      focusRunning = false;
      focusEndsAt = null;
      updateFocusTrayTooltip();
      return;
    }
    focusEndsAt = Date.now() + focusRemainingMs;
    focusRunning = true;
    updateFocusTrayTooltip();
  }

  function toggleFocusFromTray() {
    if (focusCompletionVisible) return;
    if (!focusRunning && focusEndsAt === null && focusRemainingMs === focusDurations[focusPhase]) {
      startFocusSession(focusPhase);
      return;
    }
    toggleFocusRunning();
  }

  function addFocusFiveMinutes() {
    if (focusCompletionVisible) return;
    focusRemainingMs += 5 * 60 * 1000;
    if (focusRunning) {
      focusEndsAt = Date.now() + focusRemainingMs;
    }
  }

  function skipFocusPhase() {
    clearFocusCompletionTimer();
    focusCompletionVisible = false;
    startFocusSession(focusPhase === "focus" ? "break" : "focus");
  }

  async function endFocusSession() {
    clearFocusCompletionTimer();
    focusCompletionVisible = false;
    focusRunning = false;
    focusPhase = "focus";
    focusEndsAt = null;
    focusRemainingMs = focusDurations.focus;
    clearFocusTarget();
    restoreTrayTaskTooltip();
    await hideFocusWindow();
  }

  async function completeFocusTarget() {
    if (!focusTarget || completingTarget) return;
    completingTarget = true;
    try {
      if (isTauri()) {
        await invoke("set_todo_completed_by_uuid", {
          uuid: focusTarget.uuid,
          completed: true,
        });
      }
      clearFocusCompletionTimer();
      focusCompletionVisible = false;
      focusRunning = false;
      focusPhase = "focus";
      focusEndsAt = null;
      focusRemainingMs = focusDurations.focus;
      clearFocusTarget();
      restoreTrayTaskTooltip();
      await hideFocusWindow();
    } finally {
      completingTarget = false;
    }
  }

  function scheduleNextFocusPhase(completedPhase: FocusPhase) {
    clearFocusCompletionTimer();
    focusCompletionTimer = window.setTimeout(() => {
      focusCompletionTimer = null;
      if (!focusCompletionVisible || focusRunning) return;
      focusCompletionVisible = false;
      focusPhase = completedPhase === "focus" ? "break" : "focus";
      focusRemainingMs = focusDurations[focusPhase];
      updateFocusTrayTooltip();
    }, 1800);
  }

  function clearFocusCompletionTimer() {
    if (focusCompletionTimer === null) return;
    window.clearTimeout(focusCompletionTimer);
    focusCompletionTimer = null;
  }

  async function hideFocusWindow() {
    if (!isTauri()) {
      window.close();
      return;
    }
    await invoke("hide_focus_window");
  }

  function toggleCompactMode() {
    compactMode = !compactMode;
    resizeFocusWindow(compactMode);
  }

  function resizeFocusWindow(compact: boolean) {
    if (!isTauri()) return;
    void invoke("set_focus_window_compact", { compact }).catch(() => {});
  }

  function startWindowDrag(event: MouseEvent) {
    if (!isTauri() || event.button !== 0) return;
    if (event.target instanceof Element && event.target.closest("button")) {
      return;
    }
    event.preventDefault();
    void getCurrentWindow().startDragging();
  }

  function formatFocusTime(milliseconds: number) {
    const totalSeconds = Math.ceil(milliseconds / 1000);
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds % 60;
    return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }
</script>

<main
  class:compact={compactMode}
  class:completed={focusCompletionVisible}
  class:resting={focusPhase === "break" && !focusCompletionVisible}
  class="focus-window-shell"
>
  <header class="focus-window-header" role="presentation" onmousedown={startWindowDrag}>
    <div>
      <p>{$translator("focus.timerName")}</p>
      <h1>{focusDisplayPhase}</h1>
    </div>
    <div class="focus-window-header-actions">
      <button
        type="button"
        aria-label={compactMode ? $translator("focus.expandWindow") : $translator("focus.compactWindow")}
        onclick={toggleCompactMode}
      >{compactMode ? $translator("focus.expand") : $translator("focus.compact")}</button>
      <button type="button" aria-label={$translator("focus.closeWindow")} onclick={() => void hideFocusWindow()}>×</button>
    </div>
  </header>

  <section class="focus-window-body" role="presentation" onmousedown={startWindowDrag}>
    <img class="focus-window-illustration" src={focusIllustrationSrc} alt="" aria-hidden="true" />

    <strong>{focusDisplayTime}</strong>
    {#if focusTarget}
      <div class="focus-window-target-row">
        <small class="focus-window-target">{$translator("focus.currentTarget", { title: focusTarget.title })}</small>
        <button type="button" onclick={() => void completeFocusTarget()} disabled={completingTarget}>
          {completingTarget ? $translator("focus.completing") : $translator("focus.completed")}
        </button>
      </div>
    {/if}
    <p>{focusDisplayHint}</p>
  </section>

  <div class="focus-window-actions">
    <button type="button" class="primary" onclick={() => {
      if (focusRemainingMs === focusDurations[focusPhase] && !focusRunning) {
        startFocusSession(focusPhase);
      } else {
        toggleFocusRunning();
      }
    }}>{focusRunning
      ? $translator("focus.pause")
      : focusRemainingMs === focusDurations[focusPhase]
        ? $translator("focus.start")
        : $translator("focus.resume")}</button>
    <button type="button" onclick={addFocusFiveMinutes}>{$translator("focus.addFiveMinutes")}</button>
    <button type="button" onclick={skipFocusPhase}>{$translator("focus.skip")}</button>
    <button type="button" onclick={() => void endFocusSession()}>{$translator("focus.end")}</button>
  </div>
</main>
