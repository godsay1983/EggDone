import { get, writable } from "svelte/store";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { currentMonitor, primaryMonitor, getCurrentWindow, LogicalSize, PhysicalPosition } from "@tauri-apps/api/window";
import { fitWindow, nextZoom, normalizeWindowPreferences, WINDOW_PREFERENCES_KEY, WINDOW_PRESETS, type WindowPreferences } from "$lib/utils/windowPreferences";

export const windowPreferences = writable<WindowPreferences>({ ...WINDOW_PRESETS.small });
export const windowPreferenceError = writable(false);
export const windowPreferenceBusy = writable(false);
let queue: Promise<void> = Promise.resolve();
let applying = false;
let displaySignature = "";

function monitorSignature(monitor: NonNullable<Awaited<ReturnType<typeof currentMonitor>>>) {
  return JSON.stringify([monitor.scaleFactor, monitor.workArea]);
}

async function applyPreferences(prefs: WindowPreferences) {
  if (!isTauri()) {
    document.documentElement.style.setProperty("zoom", String(prefs.zoom));
    return;
  }
  const win = getCurrentWindow();
  const monitor = await currentMonitor() ?? await primaryMonitor();
  if (!monitor) throw new Error("No display available");
  const scale = monitor.scaleFactor;
  const area = monitor.workArea;
  const fitted = fitWindow(prefs, { width: area.size.width / scale, height: area.size.height / scale });
  // Clear old constraints before switching displays or reducing the zoom level.
  await win.setMinSize(null);
  await win.setMaxSize(null);
  const currentSize = await win.innerSize();
  if (Math.abs(currentSize.width / scale - fitted.width) > 1 || Math.abs(currentSize.height / scale - fitted.height) > 1) {
    await win.setSize(new LogicalSize(fitted.width, fitted.height));
  }
  await win.setMinSize(new LogicalSize(fitted.minWidth, fitted.minHeight));
  await win.setMaxSize(new LogicalSize(fitted.maxWidth, fitted.maxHeight));
  await getCurrentWebview().setZoom(prefs.zoom);
  const position = await win.outerPosition();
  const size = await win.outerSize();
  const margin = 8 * scale;
  const x = Math.max(area.position.x + margin, Math.min(position.x, area.position.x + area.size.width - size.width - margin));
  const y = Math.max(area.position.y + margin, Math.min(position.y, area.position.y + area.size.height - size.height - margin));
  if (x !== position.x || y !== position.y) await win.setPosition(new PhysicalPosition(x, y));
  displaySignature = monitorSignature(monitor);
}

export function updateWindowPreferences(change: Partial<WindowPreferences>) {
  queue = queue.then(async () => {
    const previous = get(windowPreferences);
    const next = normalizeWindowPreferences({ ...previous, ...change });
    applying = true;
    windowPreferenceBusy.set(true);
    windowPreferenceError.set(false);
    try {
      await applyPreferences(next);
      localStorage.setItem(WINDOW_PREFERENCES_KEY, JSON.stringify(next));
      windowPreferences.set(next);
    } catch {
      windowPreferenceError.set(true);
      try { await applyPreferences(previous); } catch { /* Keep the failure visible in settings. */ }
    } finally {
      applying = false;
      windowPreferenceBusy.set(false);
    }
  });
  return queue;
}

export async function initializeWindowPreferences(): Promise<() => void> {
  try {
    windowPreferences.set(normalizeWindowPreferences(JSON.parse(localStorage.getItem(WINDOW_PREFERENCES_KEY) ?? "null")));
  } catch { windowPreferences.set({ ...WINDOW_PRESETS.small }); }
  await updateWindowPreferences({});
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let displayTimer: ReturnType<typeof setTimeout> | undefined;
  const unlisten: Array<() => void> = [];
  const keydown = (event: KeyboardEvent) => {
    if (!(event.ctrlKey || event.metaKey) || event.altKey || event.isComposing) return;
    if (!["+", "=", "-", "0"].includes(event.key)) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    void updateWindowPreferences({ zoom: event.key === "0" ? 1 : nextZoom(get(windowPreferences).zoom, event.key === "-" ? -1 : 1) });
  };
  window.addEventListener("keydown", keydown, true);
  if (isTauri()) {
    const win = getCurrentWindow();
    const saveSize = async () => {
      clearTimeout(timer);
      timer = undefined;
      if (disposed || applying) return;
      try {
        const [size, scale] = await Promise.all([win.innerSize(), win.scaleFactor()]);
        if (disposed || applying) return;
        const prefs = normalizeWindowPreferences({ ...get(windowPreferences), width: size.width / scale, height: size.height / scale });
        localStorage.setItem(WINDOW_PREFERENCES_KEY, JSON.stringify(prefs));
        windowPreferences.set(prefs);
      } catch { windowPreferenceError.set(true); }
    };
    try {
      unlisten.push(await win.onResized(() => {
        if (applying) return;
        clearTimeout(timer);
        timer = setTimeout(() => { void saveSize(); }, 250);
      }));
      unlisten.push(await win.onScaleChanged(() => { void updateWindowPreferences({}); }));
      unlisten.push(await win.onFocusChanged(({ payload }) => {
        void (async () => {
          // Flush a just-finished resize before a quick tray hide/show can restore old dimensions.
          if (timer !== undefined) await saveSize();
          if (payload && !disposed && !applying) await updateWindowPreferences({});
        })().catch(() => windowPreferenceError.set(true));
      }));
      unlisten.push(await win.onMoved(() => {
        clearTimeout(displayTimer);
        displayTimer = setTimeout(() => {
          void (async () => {
            if (disposed || applying) return;
            const monitor = await currentMonitor();
            if (!disposed && monitor && monitorSignature(monitor) !== displaySignature) await updateWindowPreferences({});
          })().catch(() => windowPreferenceError.set(true));
        }, 250);
      }));
    } catch { windowPreferenceError.set(true); }
  }
  return () => {
    disposed = true;
    clearTimeout(timer);
    clearTimeout(displayTimer);
    unlisten.forEach(stop => stop());
    window.removeEventListener("keydown", keydown, true);
  };
}
