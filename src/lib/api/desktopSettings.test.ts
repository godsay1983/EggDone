import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), register: vi.fn(), unregister: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: () => true }));
vi.mock("@tauri-apps/plugin-global-shortcut", () => ({ register: mocks.register, unregister: mocks.unregister }));
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: vi.fn(), disable: vi.fn(), isEnabled: vi.fn(async () => false),
}));

interface Preference { shortcut: string; enabled: boolean }
let saved: Partial<Record<string, Preference>>;
let legacy: Map<string, string>;
let registered: Set<string>;
let failRead: boolean;
let failSave: boolean;
let occupied: string | null;

beforeEach(() => {
  vi.resetModules();
  vi.clearAllMocks();
  saved = {};
  legacy = new Map();
  registered = new Set();
  failRead = false;
  failSave = false;
  occupied = null;
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => legacy.get(key) ?? null,
    setItem: (key: string, value: string) => legacy.set(key, value),
  });
  mocks.invoke.mockImplementation(async (command: string, args: { kind: string; preference?: Preference }) => {
    if (command === "get_shortcut_preference") {
      if (failRead) throw Error("read failed");
      return saved[args.kind] ?? null;
    }
    if (command === "save_shortcut_preference") {
      if (failSave) throw Error("write failed");
      saved[args.kind] = { ...args.preference! };
    }
  });
  mocks.register.mockImplementation(async (shortcut: string) => {
    if (shortcut === occupied || registered.has(shortcut)) throw Error("occupied");
    registered.add(shortcut);
  });
  mocks.unregister.mockImplementation(async (shortcut: string) => { registered.delete(shortcut); });
});

describe("desktop shortcut persistence", () => {
  it("migrates legacy settings and restores them after a fresh runtime without localStorage", async () => {
    legacy.set("eggdone-note-shortcut", "Alt+Shift+N");
    legacy.set("eggdone-note-shortcut-enabled", "true");
    const first = await import("./desktopSettings");
    await first.initializeDesktopSettings();
    await first.updateNoteShortcut("CommandOrControl+Alt+N", true);
    legacy.clear();
    registered.clear();
    vi.resetModules();
    const second = await import("./desktopSettings");
    const settings = await second.initializeDesktopSettings();
    expect(settings.noteShortcut).toBe("CommandOrControl+Alt+N");
    expect(settings.noteShortcutEnabled).toBe(true);
    expect(registered.has(settings.noteShortcut)).toBe(true);
  });

  it("persists the disabled preference and does not register it on restart", async () => {
    const first = await import("./desktopSettings");
    await first.initializeDesktopSettings();
    await first.updateNoteShortcut("Alt+Shift+N", false);
    registered.clear();
    vi.resetModules();
    const settings = await (await import("./desktopSettings")).initializeDesktopSettings();
    expect(settings.noteShortcut).toBe("Alt+Shift+N");
    expect(settings.noteShortcutEnabled).toBe(false);
    expect(registered.has("Alt+Shift+N")).toBe(false);
  });

  it("retains enabled intent on registration failure and retries on a later startup", async () => {
    saved.note = { shortcut: "Alt+Shift+N", enabled: true };
    occupied = "Alt+Shift+N";
    const api = await import("./desktopSettings");
    const settings = await api.initializeDesktopSettings();
    expect(settings.noteShortcutEnabled).toBe(true);
    expect(settings.noteShortcutError).toContain("occupied");
    expect(saved.note.enabled).toBe(true);
    occupied = null;
    const retried = await api.initializeDesktopSettings();
    expect(retried.noteShortcutError).toBeNull();
    expect(registered.has("Alt+Shift+N")).toBe(true);
  });

  it("never replaces a native read failure with defaults", async () => {
    saved.note = { shortcut: "Alt+Shift+N", enabled: true };
    failRead = true;
    await expect((await import("./desktopSettings")).initializeDesktopSettings()).rejects.toThrow("read failed");
    expect(mocks.invoke.mock.calls.some(([command]) => command === "save_shortcut_preference")).toBe(false);
    expect(saved.note.enabled).toBe(true);
  });

  it("rolls back registration and storage when a new binding is occupied", async () => {
    saved.note = { shortcut: "Alt+Shift+N", enabled: true };
    const api = await import("./desktopSettings");
    await api.initializeDesktopSettings();
    occupied = "CommandOrControl+Alt+N";
    await expect(api.updateNoteShortcut(occupied, true)).rejects.toThrow("occupied");
    expect(registered.has("Alt+Shift+N")).toBe(true);
    expect(saved.note.shortcut).toBe("Alt+Shift+N");
  });

  it("rolls back runtime registration when saving fails, including disabling", async () => {
    saved.note = { shortcut: "Alt+Shift+N", enabled: true };
    const api = await import("./desktopSettings");
    await api.initializeDesktopSettings();
    failSave = true;
    await expect(api.updateNoteShortcut("CommandOrControl+Alt+N", true)).rejects.toThrow("write failed");
    expect(registered.has("CommandOrControl+Alt+N")).toBe(false);
    expect(registered.has("Alt+Shift+N")).toBe(true);
    await expect(api.updateNoteShortcut("Alt+Shift+N", false)).rejects.toThrow("write failed");
    expect(registered.has("Alt+Shift+N")).toBe(true);
    expect(saved.note).toEqual({ shortcut: "Alt+Shift+N", enabled: true });
  });

  it("persists the panel shortcut independently and retains enabled intent if occupied", async () => {
    const api = await import("./desktopSettings");
    const old = await api.initializeDesktopSettings();
    await api.updateShortcut(old.shortcut, old.shortcutEnabled, "Alt+Shift+Space", true);
    expect(saved.note?.enabled).toBe(false);
    occupied = "Alt+Shift+Space";
    registered.clear();
    vi.resetModules();
    const settings = await (await import("./desktopSettings")).initializeDesktopSettings();
    expect(settings.shortcut).toBe("Alt+Shift+Space");
    expect(settings.shortcutEnabled).toBe(true);
    expect(settings.shortcutError).toContain("occupied");
  });

  it("only dispatches quick capture on key press and does not register twice", async () => {
    saved.note = { shortcut: "Alt+Shift+N", enabled: true };
    const api = await import("./desktopSettings");
    await api.initializeDesktopSettings();
    await api.initializeDesktopSettings();
    const calls = mocks.register.mock.calls.filter(([shortcut]) => shortcut === "Alt+Shift+N");
    expect(calls).toHaveLength(1);
    await calls[0][1]({ state: "Released" });
    await calls[0][1]({ state: "Pressed" });
    expect(mocks.invoke.mock.calls.filter(([command]) => command === "quick_capture_note")).toHaveLength(1);
  });
});
