import { getCurrentWindow } from '@tauri-apps/api/window';
import { initializeAutoSync, setAutoSyncForeground, syncOnPanelShown } from './autoSync';

export function bindAutoSyncWindow(onError: (reason: unknown) => void): () => void {
  let disposed = false;
  let eventVersion = 0;
  const unlisteners: (() => void)[] = [];
  const retain = (unlisten: () => void) => {
    if (disposed) unlisten();
    else unlisteners.push(unlisten);
  };

  void initializeAutoSync().then(async () => {
    if (disposed) return;
    const appWindow = getCurrentWindow();
    retain(await appWindow.listen('panel-shown', () => {
      if (disposed) return;
      eventVersion += 1;
      syncOnPanelShown();
    }));
    if (disposed) return;
    retain(await appWindow.onFocusChanged(({ payload }) => {
      if (disposed) return;
      eventVersion += 1;
      setAutoSyncForeground(payload);
    }));
    if (disposed) return;
    // Subscribe before the initial query so a late visibility result cannot
    // overwrite a newer native show/focus event.
    const version = eventVersion;
    const visible = await appWindow.isVisible();
    if (disposed || version !== eventVersion) return;
    if (visible) syncOnPanelShown();
    else setAutoSyncForeground(false);
  }).catch((reason) => {
    if (!disposed) onError(reason);
  });

  return () => {
    disposed = true;
    unlisteners.forEach((unlisten) => unlisten());
    setAutoSyncForeground(false);
  };
}
