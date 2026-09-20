import { beforeEach, describe, expect, it, vi } from 'vitest';
import { bindAutoSyncWindow } from './autoSyncWindow';
import { initializeAutoSync, setAutoSyncForeground, syncOnPanelShown } from './autoSync';

const native = vi.hoisted(() => ({
  listen: vi.fn(), onFocusChanged: vi.fn(), isVisible: vi.fn(),
}));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => native }));
vi.mock('./autoSync', () => ({
  initializeAutoSync: vi.fn(), setAutoSyncForeground: vi.fn(), syncOnPanelShown: vi.fn(),
}));

let shown: () => void;
let focused: (event: { payload: boolean }) => void;
let stopShown: () => void;
let stopFocus: () => void;
const settle = async () => { for (let i = 0; i < 10; i++) await Promise.resolve(); };

beforeEach(() => {
  vi.resetAllMocks();
  stopShown = vi.fn(); stopFocus = vi.fn();
  vi.mocked(initializeAutoSync).mockResolvedValue();
  native.listen.mockImplementation(async (_event: string, listener: () => void) => {
    shown = listener;
    return stopShown;
  });
  native.onFocusChanged.mockImplementation(async (listener: typeof focused) => {
    focused = listener;
    return stopFocus;
  });
  native.isVisible.mockResolvedValue(false);
});

describe('native panel sync binding', () => {
  it('syncs a tray show even without any focus notification', async () => {
    const dispose = bindAutoSyncWindow(vi.fn());
    await settle();
    expect(native.listen).toHaveBeenCalledWith('panel-shown', expect.any(Function));
    expect(syncOnPanelShown).not.toHaveBeenCalled();
    shown();
    expect(syncOnPanelShown).toHaveBeenCalledTimes(1);
    focused({ payload: false });
    expect(setAutoSyncForeground).toHaveBeenLastCalledWith(false);
    dispose();
    expect(stopShown).toHaveBeenCalledOnce();
    expect(stopFocus).toHaveBeenCalledOnce();
    shown();
    expect(syncOnPanelShown).toHaveBeenCalledTimes(1);
  });

  it('syncs a panel already visible when listeners finish registering', async () => {
    native.isVisible.mockResolvedValue(true);
    const dispose = bindAutoSyncWindow(vi.fn());
    await settle();
    expect(syncOnPanelShown).toHaveBeenCalledOnce();
    dispose();
  });

  it('does not let a late visibility query undo a native show event', async () => {
    let resolve!: (value: boolean) => void;
    native.isVisible.mockReturnValue(new Promise<boolean>((yes) => { resolve = yes; }));
    const dispose = bindAutoSyncWindow(vi.fn());
    await settle();
    shown();
    resolve(false);
    await settle();
    expect(syncOnPanelShown).toHaveBeenCalledOnce();
    expect(setAutoSyncForeground).not.toHaveBeenCalled();
    dispose();
  });

  it('does not register or restart synchronization after disposal during initialization', async () => {
    let resolve!: () => void;
    vi.mocked(initializeAutoSync).mockReturnValue(new Promise<void>((yes) => { resolve = yes; }));
    const dispose = bindAutoSyncWindow(vi.fn());
    dispose();
    resolve();
    await settle();
    expect(native.listen).not.toHaveBeenCalled();
    expect(syncOnPanelShown).not.toHaveBeenCalled();
  });

  it('releases a listener whose registration completes after disposal', async () => {
    let resolve!: (value: () => void) => void;
    native.listen.mockReturnValue(new Promise<() => void>((yes) => { resolve = yes; }));
    const dispose = bindAutoSyncWindow(vi.fn());
    await settle();
    dispose();
    resolve(stopShown);
    await settle();
    expect(stopShown).toHaveBeenCalledOnce();
    expect(native.onFocusChanged).not.toHaveBeenCalled();
  });

  it('reports native setup errors', async () => {
    const error = new Error('native window unavailable');
    native.isVisible.mockRejectedValue(error);
    const report = vi.fn();
    const dispose = bindAutoSyncWindow(report);
    await settle();
    expect(report).toHaveBeenCalledWith(error);
    dispose();
  });
});
