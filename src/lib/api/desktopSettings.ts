import { invoke, isTauri } from "@tauri-apps/api/core";
import { get } from "svelte/store";
import { translator } from "$lib/i18n";
import {
  register,
  unregister,
  isRegistered,
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

export type CapabilityStatus = "unknown" | "enabled" | "disabled" | "inactive" | "unsupported";
export interface DesktopSettings {
  shortcutPreferenceKnown: boolean;
  noteShortcutPreferenceKnown: boolean;
  shortcutStatus: CapabilityStatus;
  noteShortcutStatus: CapabilityStatus;
  autostartStatus: CapabilityStatus;
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

async function readPreference(kind: ShortcutKind, migrate = true): Promise<ShortcutPreference> {
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
  if (migrate) await savePreference(kind, preference);
  return preference;
}

export async function initializeDesktopSettings(): Promise<DesktopSettings> {
  return readDesktopSettings(true);
}

/** Opening settings and returning to the app must never re-enable an OS capability. */
export async function refreshDesktopSettings(): Promise<DesktopSettings> {
  return readDesktopSettings(false);
}

async function readDesktopSettings(initialize: boolean): Promise<DesktopSettings> {
  const panel = await readShortcutCapability("panel", initialize);
  const note = await readShortcutCapability("note", initialize);
  let autostartEnabled = false;
  let autostartStatus: CapabilityStatus = isTauri() ? "unknown" : "unsupported";
  let autostartError: string | null = null;
  if (isTauri()) {
    try {
      autostartEnabled = await isAutostartEnabled();
      autostartStatus = autostartEnabled ? "enabled" : "disabled";
    } catch (error) { autostartError = capabilityReadError(error); }
  }
  return {
    shortcut: panel.shortcut, shortcutEnabled: panel.enabled,
    shortcutPreferenceKnown: panel.known, shortcutStatus: panel.status, shortcutError: panel.error,
    noteShortcut: note.shortcut, noteShortcutEnabled: note.enabled,
    noteShortcutPreferenceKnown: note.known, noteShortcutStatus: note.status, noteShortcutError: note.error,
    autostartEnabled, autostartStatus, autostartError,
  };
}

async function readShortcutCapability(kind: ShortcutKind, initialize: boolean) {
  let preference: ShortcutPreference = {
    shortcut: (kind === "note" ? noteShortcutOptions : shortcutOptions)[0].value, enabled: false,
  };
  let known = false;
  let status: CapabilityStatus = isTauri() ? "unknown" : "unsupported";
  let error: string | null = null;
  try {
    preference = await readPreference(kind, initialize);
    known = true;
  } catch (reason) { error = capabilityReadError(reason); }
  if (known && isTauri()) {
    if (initialize && preference.enabled) {
      try {
        if (kind === "note") await registerNoteShortcut(preference.shortcut);
        else await registerShortcut(preference.shortcut);
      } catch (reason) { error = shortcutErrorMessage(reason); }
    }
    try {
      const registered = await isRegistered(preference.shortcut);
      status = registered ? "enabled" : preference.enabled ? "inactive" : "disabled";
      // Drop stale local handles, but do not silently register on refresh.
      if (!registered && kind === "note" && activeNoteShortcut === preference.shortcut) activeNoteShortcut = null;
      if (!registered && kind === "panel" && activeShortcut === preference.shortcut) activeShortcut = null;
    } catch (reason) { error = capabilityReadError(reason); }
  }
  return { ...preference, known, status, error };
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

function capabilityReadError(error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  return get(translator)("settings.capabilityReadFailed", { detail });
}
