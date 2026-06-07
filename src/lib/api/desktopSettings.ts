import { invoke } from "@tauri-apps/api/core";
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

export const shortcutOptions = [
  { value: "CommandOrControl+Shift+Space", label: "Ctrl + Shift + Space" },
  { value: "CommandOrControl+Alt+Space", label: "Ctrl + Alt + Space" },
  { value: "Alt+Shift+Space", label: "Alt + Shift + Space" },
  { value: "CommandOrControl+Shift+E", label: "Ctrl + Shift + E" },
] as const;

export interface DesktopSettings {
  shortcut: string;
  shortcutEnabled: boolean;
  autostartEnabled: boolean;
  shortcutError: string | null;
  autostartError: string | null;
}

let activeShortcut: string | null = null;

export async function initializeDesktopSettings(): Promise<DesktopSettings> {
  const shortcut =
    localStorage.getItem(SHORTCUT_KEY) ?? shortcutOptions[0].value;
  const shortcutEnabled =
    localStorage.getItem(SHORTCUT_ENABLED_KEY) !== "false";
  let shortcutError: string | null = null;
  let autostartEnabled = false;
  let autostartError: string | null = null;

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

  return {
    shortcut,
    shortcutEnabled: shortcutEnabled && shortcutError === null,
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
  if (activeShortcut) {
    await unregister(activeShortcut);
    activeShortcut = null;
  }

  if (enabled) {
    try {
      await registerShortcut(shortcut);
    } catch (error) {
      if (previousEnabled && previousShortcut) {
        try {
          await registerShortcut(previousShortcut);
        } catch {
          activeShortcut = null;
        }
      }
      throw new Error(shortcutErrorMessage(error));
    }
  }

  localStorage.setItem(SHORTCUT_KEY, shortcut);
  localStorage.setItem(SHORTCUT_ENABLED_KEY, String(enabled));
}

export async function updateAutostart(enabled: boolean): Promise<boolean> {
  if (enabled) {
    await enableAutostart();
  } else {
    await disableAutostart();
  }
  return isAutostartEnabled();
}

async function registerShortcut(shortcut: string) {
  await register(shortcut, async (event) => {
    if (event.state === "Pressed") {
      await invoke("toggle_panel_from_shortcut");
    }
  });
  activeShortcut = shortcut;
}

function shortcutErrorMessage(error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  return `快捷键注册失败，可能已被其他程序占用：${detail}`;
}

function settingErrorMessage(message: string, error: unknown) {
  const detail = error instanceof Error ? error.message : String(error);
  return `${message}：${detail}`;
}
