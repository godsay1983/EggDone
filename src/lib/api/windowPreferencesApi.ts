import { invoke, isTauri } from "@tauri-apps/api/core";
import { normalizeWindowPreferences, WINDOW_PREFERENCES_KEY, type WindowPreferences } from "$lib/utils/windowPreferences";

export async function readWindowPreferences(): Promise<WindowPreferences> {
  if (isTauri()) {
    const saved = await invoke<WindowPreferences | null>("get_window_preferences");
    if (saved !== null) return normalizeWindowPreferences(saved);
  }
  // Migrate the old WebView-only preference once; native read failures must not
  // silently replace a durable preference with defaults.
  try {
    return normalizeWindowPreferences(JSON.parse(localStorage.getItem(WINDOW_PREFERENCES_KEY) ?? "null"));
  } catch {
    return normalizeWindowPreferences(null);
  }
}

export async function saveWindowPreferences(preferences: WindowPreferences): Promise<void> {
  if (isTauri()) {
    await invoke("save_window_preferences", { preferences });
    return;
  }
  localStorage.setItem(WINDOW_PREFERENCES_KEY, JSON.stringify(preferences));
}
