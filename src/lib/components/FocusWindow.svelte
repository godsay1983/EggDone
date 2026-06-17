<script lang="ts">
  import { invoke, isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
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
  let focusDisplayHint = "开始一颗番茄，先把注意力放在眼前这一件事。";
  let focusDisplayPhase = "专注";
  let focusTarget: FocusTarget | null = getFocusTarget();
  let completingTarget = false;

  $: focusDisplayPhase = focusPhase === "focus" ? "专注" : "休息";
  $: focusDisplayTime = formatFocusTime(focusRemainingMs);
  $: focusDisplayHint = focusRunning
    ? "保持当前节奏，结束后会切到下一阶段。"
    : focusRemainingMs === focusDurations[focusPhase]
      ? "开始一颗番茄，先把注意力放在眼前这一件事。"
      : "已经暂停，继续时会从当前剩余时间开始。";

  onMount(() => {
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
    return () => {
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
    refreshFocusDurations();
    focusPhase = phase;
    focusRemainingMs = focusDurations[phase];
    focusEndsAt = Date.now() + focusRemainingMs;
    focusRunning = true;
  }

  function updateFocusTimer() {
    if (!focusRunning || focusEndsAt === null) return;
    focusRemainingMs = Math.max(0, focusEndsAt - Date.now());
    if (focusRemainingMs > 0) return;
    const completedPhase = focusPhase;
    focusRunning = false;
    focusEndsAt = null;
    focusPhase = focusPhase === "focus" ? "break" : "focus";
    focusRemainingMs = focusDurations[focusPhase];
    if (isTauri()) {
      void invoke("publish_focus_notification", { completedPhase }).catch(() => {});
    }
  }

  function toggleFocusRunning() {
    if (focusRunning) {
      updateFocusTimer();
      focusRunning = false;
      focusEndsAt = null;
      return;
    }
    focusEndsAt = Date.now() + focusRemainingMs;
    focusRunning = true;
  }

  function addFocusFiveMinutes() {
    focusRemainingMs += 5 * 60 * 1000;
    if (focusRunning) {
      focusEndsAt = Date.now() + focusRemainingMs;
    }
  }

  function skipFocusPhase() {
    startFocusSession(focusPhase === "focus" ? "break" : "focus");
  }

  async function endFocusSession() {
    focusRunning = false;
    focusPhase = "focus";
    focusEndsAt = null;
    focusRemainingMs = focusDurations.focus;
    clearFocusTarget();
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
      focusRunning = false;
      focusPhase = "focus";
      focusEndsAt = null;
      focusRemainingMs = focusDurations.focus;
      clearFocusTarget();
      await hideFocusWindow();
    } finally {
      completingTarget = false;
    }
  }

  async function hideFocusWindow() {
    if (!isTauri()) {
      window.close();
      return;
    }
    await invoke("hide_focus_window");
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

<main class:resting={focusPhase === "break"} class="focus-window-shell">
  <header class="focus-window-header" role="presentation" onmousedown={startWindowDrag}>
    <div>
      <p>番茄钟</p>
      <h1>{focusDisplayPhase}</h1>
    </div>
    <button type="button" aria-label="关闭专注窗口" onclick={() => void hideFocusWindow()}>×</button>
  </header>

  <section class="focus-window-body" role="presentation" onmousedown={startWindowDrag}>
    <img class="focus-window-illustration" src="/focus-illustration.png" alt="" aria-hidden="true" />

    <strong>{focusDisplayTime}</strong>
    {#if focusTarget}
      <div class="focus-window-target-row">
        <small class="focus-window-target">正在专注：{focusTarget.title}</small>
        <button type="button" onclick={() => void completeFocusTarget()} disabled={completingTarget}>
          {completingTarget ? "完成中" : "完成"}
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
    }}>{focusRunning ? "暂停" : "开始"}</button>
    <button type="button" onclick={addFocusFiveMinutes}>+5 分钟</button>
    <button type="button" onclick={skipFocusPhase}>跳过</button>
    <button type="button" onclick={() => void endFocusSession()}>结束</button>
  </div>
</main>
