export const WINDOW_PREFERENCES_KEY = "eggdone-window-preferences-v1";
export const ZOOM_LEVELS = [1, 1.15, 1.25, 1.5] as const;
export interface WindowPreferences { width: number; height: number; zoom: number }
export const WINDOW_PRESETS = {
  small: { width: 360, height: 560, zoom: 1 },
  comfortable: { width: 480, height: 680, zoom: 1.25 },
  large: { width: 640, height: 820, zoom: 1.5 },
} satisfies Record<string, WindowPreferences>;

export function normalizeWindowPreferences(value: unknown): WindowPreferences {
  const raw = value && typeof value === "object" ? value as Partial<WindowPreferences> : {};
  const dimension = (value: unknown, fallback: number, min: number) =>
    typeof value === "number" && Number.isFinite(value)
      ? Math.round(Math.max(min, Math.min(4096, value))) : fallback;
  return {
    width: dimension(raw.width, 360, 360),
    height: dimension(raw.height, 560, 420),
    zoom: ZOOM_LEVELS.find(level => level === raw.zoom) ?? 1,
  };
}

export function fitWindow(prefs: WindowPreferences, available: { width: number; height: number }) {
  const maxWidth = Math.max(1, Math.floor(available.width - 16));
  const maxHeight = Math.max(1, Math.floor(available.height - 16));
  const minWidth = Math.min(maxWidth, Math.ceil(360 * prefs.zoom));
  const minHeight = Math.min(maxHeight, Math.ceil(420 * prefs.zoom));
  return {
    minWidth, minHeight, maxWidth, maxHeight,
    width: Math.min(maxWidth, Math.max(minWidth, prefs.width)),
    height: Math.min(maxHeight, Math.max(minHeight, prefs.height)),
  };
}

export function nextZoom(current: number, direction: number): number {
  const index = Math.max(0, ZOOM_LEVELS.findIndex(level => level === current));
  return ZOOM_LEVELS[Math.min(ZOOM_LEVELS.length - 1, Math.max(0, index + direction))];
}
