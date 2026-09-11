import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), register: vi.fn(), unregister: vi.fn(), isRegistered: vi.fn(),
  enableAutostart: vi.fn(), disableAutostart: vi.fn(), readAutostart: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke, isTauri: () => true }));
vi.mock("@tauri-apps/plugin-global-shortcut", () => ({ register: mocks.register, unregister: mocks.unregister, isRegistered: mocks.isRegistered }));
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: mocks.enableAutostart, disable: mocks.disableAutostart, isEnabled: mocks.readAutostart,
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
  mocks.enableAutostart.mockReset().mockResolvedValue(undefined);
  mocks.disableAutostart.mockReset().mockResolvedValue(undefined);
  mocks.readAutostart.mockReset().mockResolvedValue(false);
  saved = {};
  legacy = new Map();
  registered = new Set();
  mocks.isRegistered.mockReset().mockImplementation(async (key: string) => registered.has(key));
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
  it("refreshes actual status without re-registering or writing preferences", async () => {
    const api = await import("./desktopSettings");
    const initial = await api.initializeDesktopSettings();
    registered.clear();
    mocks.invoke.mockClear(); mocks.register.mockClear();
    mocks.readAutostart.mockResolvedValue(true);
    const current = await api.refreshDesktopSettings();
    expect(current.shortcutEnabled).toBe(initial.shortcutEnabled);
    expect(current.shortcutStatus).toBe("inactive");
    expect(current.autostartStatus).toBe("enabled");
    expect(mocks.register).not.toHaveBeenCalled();
    expect(mocks.enableAutostart).not.toHaveBeenCalled();
    expect(mocks.disableAutostart).not.toHaveBeenCalled();
    expect(mocks.invoke.mock.calls.every(([command]) => command === "get_shortcut_preference")).toBe(true);
    await api.updateShortcut(current.shortcut, true, current.shortcut, true);
    expect((await api.refreshDesktopSettings()).shortcutStatus).toBe("enabled");
  });

  it("isolates one shortcut read failure and recovers with a read-only refresh", async () => {
    saved.note = { shortcut: "Alt+Shift+N", enabled: true };
    mocks.invoke.mockRejectedValueOnce(Error("panel read failed"));
    const api = await import("./desktopSettings");
    const initial = await api.initializeDesktopSettings();
    expect(initial.shortcutPreferenceKnown).toBe(false);
    expect(initial.noteShortcutStatus).toBe("enabled");
    expect(initial.autostartStatus).toBe("disabled");
    mocks.invoke.mockClear(); mocks.register.mockClear();
    const recovered = await api.refreshDesktopSettings();
    expect(recovered.shortcutPreferenceKnown).toBe(true);
    expect(recovered.shortcutStatus).toBe("inactive");
    expect(mocks.register).not.toHaveBeenCalled();
    expect(mocks.invoke.mock.calls.every(([command]) => command === "get_shortcut_preference")).toBe(true);
  });

  it("reports an OS query failure as unknown without clearing saved intent", async () => {
    const api = await import("./desktopSettings");
    await api.initializeDesktopSettings();
    mocks.isRegistered.mockRejectedValueOnce(Error("query failed"));
    const unknown = await api.refreshDesktopSettings();
    expect(unknown.shortcutStatus).toBe("unknown");
    expect(unknown.shortcutPreferenceKnown).toBe(true);
    expect(unknown.shortcutEnabled).toBe(true);
    expect(unknown.shortcutError).toContain("query failed");
    expect((await api.refreshDesktopSettings()).shortcutStatus).toBe("enabled");
  });

  it("preserves disabled intent when a stale OS registration remains", async () => {
    const api = await import("./desktopSettings");
    const initial = await api.initializeDesktopSettings();
    saved.panel!.enabled = false;
    const current = await api.refreshDesktopSettings();
    expect(current.shortcutEnabled).toBe(false);
    expect(current.shortcutStatus).toBe("enabled");
    expect(registered.has(initial.shortcut)).toBe(true);
  });

  it("reports unreadable autostart as an error without changing OS settings or shortcut intent", async () => {
    saved.note = { shortcut: "Alt+Shift+N", enabled: true };
    mocks.readAutostart.mockRejectedValue(Error("OS status unavailable"));
    const settings = await (await import("./desktopSettings")).initializeDesktopSettings();
    expect(settings.autostartError).toContain("OS status unavailable");
    expect(settings.autostartStatus).toBe("unknown");
    expect(settings.noteShortcutEnabled).toBe(true);
    expect(mocks.enableAutostart).not.toHaveBeenCalled();
    expect(mocks.disableAutostart).not.toHaveBeenCalled();
  });

  it("returns the OS readback rather than assuming requested autostart is active", async () => {
    const api = await import("./desktopSettings");
    expect(await api.updateAutostart(true)).toBe(false);
    expect(mocks.enableAutostart).toHaveBeenCalledOnce();
    mocks.readAutostart.mockResolvedValue(true);
    expect(await api.updateAutostart(false)).toBe(true);
    expect(mocks.disableAutostart).toHaveBeenCalledOnce();
  });

  it("propagates autostart operation and readback failures without claiming success", async () => {
    const api = await import("./desktopSettings");
    mocks.enableAutostart.mockRejectedValue(Error("OS denied"));
    await expect(api.updateAutostart(true)).rejects.toThrow("OS denied");
    expect(mocks.readAutostart).not.toHaveBeenCalled();
    mocks.readAutostart.mockRejectedValue(Error("readback failed"));
    await expect(api.updateAutostart(false)).rejects.toThrow("readback failed");
  });

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
    const settings = await (await import("./desktopSettings")).initializeDesktopSettings();
    expect(settings.shortcutPreferenceKnown).toBe(false);
    expect(settings.noteShortcutPreferenceKnown).toBe(false);
    expect(settings.shortcutStatus).toBe("unknown");
    expect(settings.autostartStatus).toBe("disabled");
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
