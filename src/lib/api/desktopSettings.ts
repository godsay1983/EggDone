import { invoke, isTauri } from "@tauri-apps/api/core";
import { get } from "svelte/store";
import { translator } from "$lib/i18n";
import {
  register,
  unregister,
} from "@tauri-apps/plugin-global-shortcut";
import {
  disable as disableAutostart,
  enable as enableAutostart,
  isEnabled as isAutostartEnabled,
} from "@tauri-apps/plugin-autostart";

const SHORTCUT_KEY = "eggdone-global-shortcut";
const SHORTCUT_ENABLED_KEY = "eggdone-global-shortcut-enabled";
const NOTE_SHORTCUT_KEY = "eggdone-note-shortcut";
const NOTE_SHORTCUT_ENABLED_KEY = "eggdone-note-shortcut-enabled";

export const noteShortcutOptions = [
  { value: "CommandOrControl+Shift+N", label: "Ctrl + Shift + N" },
  { value: "CommandOrControl+Alt+N", label: "Ctrl + Alt + N" },
  { value: "Alt+Shift+N", label: "Alt + Shift + N" },
] as const;

export const shortcutOptions = [
  { value: "CommandOrControl+Shift+Space", label: "Ctrl + Shift + Space" },
  { value: "CommandOrControl+Alt+Space", label: "Ctrl + Alt + Space" },
  { value: "Alt+Shift+Space", label: "Alt + Shift + Space" },
  { value: "CommandOrControl+Shift+E", label: "Ctrl + Shift + E" },
] as const;

export interface DesktopSettings {
  noteShortcut: string;
  noteShortcutEnabled: boolean;
  noteShortcutError: string | null;
  shortcut: string;
  shortcutEnabled: boolean;
  autostartEnabled: boolean;
  shortcutError: string | null;
  autostartError: string | null;
}

let activeShortcut: string | null = null;
let activeNoteShortcut: string | null = null;

type ShortcutKind = "panel" | "note";
interface ShortcutPreference { shortcut: string; enabled: boolean }

async function savePreference(kind: ShortcutKind, preference: ShortcutPreference): Promise<void> {
  if (isTauri()) {
    await invoke("save_shortcut_preference", { kind, preference });
    return;
  }
  localStorage.setItem(kind === "note" ? NOTE_SHORTCUT_KEY : SHORTCUT_KEY, preference.shortcut);
  localStorage.setItem(kind === "note" ? NOTE_SHORTCUT_ENABLED_KEY : SHORTCUT_ENABLED_KEY, String(preference.enabled));
}

async function readPreference(kind: ShortcutKind): Promise<ShortcutPreference> {
  if (isTauri()) {
    const saved = await invoke<ShortcutPreference | null>("get_shortcut_preference", { kind });
    if (saved !== null) return saved;
  }
  const options = kind === "note" ? noteShortcutOptions : shortcutOptions;
  const legacy = localStorage.getItem(kind === "note" ? NOTE_SHORTCUT_KEY : SHORTCUT_KEY);
  const enabled = localStorage.getItem(kind === "note" ? NOTE_SHORTCUT_ENABLED_KEY : SHORTCUT_ENABLED_KEY);
  const preference = {
    shortcut: options.find(option => option.value === legacy)?.value ?? options[0].value,
    enabled: kind === "note" ? enabled === "true" : enabled !== "false",
  };
  // Migrate the user's intent even if this launch cannot register the shortcut.
  // A native read error propagates instead of overwriting durable settings.
  await savePreference(kind, preference);
  return preference;
}

export async function initializeDesktopSettings(): Promise<DesktopSettings> {
  const panelPreference = await readPreference("panel");
  const notePreference = await readPreference("note");
  const { shortcut, enabled: shortcutEnabled } = panelPreference;
  let shortcutError: string | null = null;
  let autostartEnabled = false;
  let autostartError: string | null = null;
  const { shortcut: noteShortcut, enabled: noteShortcutEnabled } = notePreference;
  let noteShortcutError: string | null = null;

  if (shortcutEnabled) {
    try {
      await registerShortcut(shortcut);
    } catch (error) {
      shortcutError = shortcutErrorMessage(error);
    }
  }

  try {
    autostartEnabled = await isAutostartEnabled();
  } catch (error) {
    autostartError = settingErrorMessage("无法读取开机启动状态", error);
  }

  if (noteShortcutEnabled) {
    try { await registerNoteShortcut(noteShortcut); }
    catch (error) { noteShortcutError = shortcutErrorMessage(error); }
  }

  return {
    noteShortcut,
    noteShortcutEnabled,
    noteShortcutError,
    shortcut,
    shortcutEnabled,
    autostartEnabled,
    shortcutError,
    autostartError,
  };
}

export async function updateShortcut(
  previousShortcut: string,
  previousEnabled: boolean,
  shortcut: string,
  enabled: boolean,
): Promise<void> {
  const previousActive = activeShortcut;
  if (activeShortcut) {
    await unregister(activeShortcut);
    activeShortcut = null;
  }

  let saving = false;
  try {
    if (enabled) await registerShortcut(shortcut);
    saving = true;
    await savePreference("panel", { shortcut, enabled });
  } catch (error) {
    if (activeShortcut) {
      try { await unregister(activeShortcut); activeShortcut = null; } catch { /* Retain the actual registration for a later retry. */ }
    }
    if (previousActive && previousEnabled && previousShortcut && !activeShortcut) {
      try { await registerShortcut(previousActive); } catch { activeShortcut = null; }
    }
    throw new Error(saving ? shortcutSaveErrorMessage(error) : shortcutErrorMessage(error));
  }
}

export async function updateAutostart(enabled: boolean): Promise<boolean> {
  if (enabled) {
    await enableAutostart();
  } else {
    await disableAutostart();
  }
  return isAutostartEnabled();
}

export async function updateNoteShortcut(shortcut: string, enabled: boolean): Promise<void> {
  const previous = activeNoteShortcut;
  if (previous) {
    await unregister(previous);
    activeNoteShortcut = null;
  }
  let saving = false;
  try {
    if (enabled) await registerNoteShortcut(shortcut);
    saving = true;
    await savePreference("note", { shortcut, enabled });
  } catch (error) {
    if (activeNoteShortcut) {
      try { await unregister(activeNoteShortcut); activeNoteShortcut = null; } catch { /* Retain the actual registration for a later retry. */ }
    }
    if (previous && !activeNoteShortcut) {
      try { await registerNoteShortcut(previous); } catch { activeNoteShortcut = null; }
    }
    throw new Error(saving ? shortcutSaveErrorMessage(error) : shortcutErrorMessage(error));
  }
}

async function registerNoteShortcut(shortcut: string) {
  if (activeNoteShortcut === shortcut) return;
  await register(shortcut, async (event) => {
    if (event.state === "Pressed") await invoke("quick_capture_note");
  });
  activeNoteShortcut = shortcut;
}

async function registerShortcut(shortcut: string) {
  if (activeShortcut === shortcut) return;
  await register(shortcut, async (event) => {
    if (event.state === "Pressed") {
      await invoke("toggle_panel_from_shortcut");
    }
  });
  activeShortcut = shortcut;
}

function shortcutErrorMessage(error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  return get(translator)("settings.shortcutRegistrationFailed", { detail });
}

function shortcutSaveErrorMessage(error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  return get(translator)("settings.shortcutSaveFailed", { detail });
}

function settingErrorMessage(message: string, error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  return `${message}：${detail}`;
}
