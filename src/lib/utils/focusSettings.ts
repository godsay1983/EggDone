import { readPreference, writePreference } from './preferenceStorage';
export type FocusPhase = "focus" | "break";

export type FocusDurations = Record<FocusPhase, number>;

export interface FocusTarget {
  uuid: string;
  title: string;
}

export const FOCUS_DURATION_OPTIONS = [15, 25, 45] as const;
export const BREAK_DURATION_OPTIONS = [5, 10, 15] as const;

const FOCUS_DURATION_KEY = "eggdone-focus-duration-minutes";
const BREAK_DURATION_KEY = "eggdone-break-duration-minutes";
const FOCUS_TARGET_UUID_KEY = "eggdone-focus-target-uuid";
const FOCUS_TARGET_TITLE_KEY = "eggdone-focus-target-title";
const DEFAULT_FOCUS_MINUTES = 25;
const DEFAULT_BREAK_MINUTES = 5;

export const FOCUS_SETTINGS_CHANGED_EVENT = "eggdone-focus-settings-changed";
export const FOCUS_TARGET_CHANGED_EVENT = "eggdone-focus-target-changed";

export function getFocusDurationMinutes() {
  return readDurationMinutes(
    FOCUS_DURATION_KEY,
    DEFAULT_FOCUS_MINUTES,
    FOCUS_DURATION_OPTIONS,
  );
}

export function getBreakDurationMinutes() {
  return readDurationMinutes(
    BREAK_DURATION_KEY,
    DEFAULT_BREAK_MINUTES,
    BREAK_DURATION_OPTIONS,
  );
}

export function getFocusDurations(): FocusDurations {
  return {
    focus: getFocusDurationMinutes() * 60 * 1000,
    break: getBreakDurationMinutes() * 60 * 1000,
  };
}

export async function saveFocusDurationMinutes(minutes: number) {
  const normalized = normalizeDurationMinutes(
    minutes,
    DEFAULT_FOCUS_MINUTES,
    FOCUS_DURATION_OPTIONS,
  );
  if (!(await writePreference(FOCUS_DURATION_KEY, String(normalized)))) return getFocusDurationMinutes();
  notifyFocusSettingsChanged();
  return normalized;
}

export async function saveBreakDurationMinutes(minutes: number) {
  const normalized = normalizeDurationMinutes(
    minutes,
    DEFAULT_BREAK_MINUTES,
    BREAK_DURATION_OPTIONS,
  );
  if (!(await writePreference(BREAK_DURATION_KEY, String(normalized)))) return getBreakDurationMinutes();
  notifyFocusSettingsChanged();
  return normalized;
}

export function getFocusTarget(): FocusTarget | null {
  const uuid = readPreference(FOCUS_TARGET_UUID_KEY);
  const title = readPreference(FOCUS_TARGET_TITLE_KEY);
  if (!uuid || !title) return null;
  return { uuid, title };
}

export function saveFocusTarget(target: FocusTarget) {
  localStorage.setItem(FOCUS_TARGET_UUID_KEY, target.uuid);
  localStorage.setItem(FOCUS_TARGET_TITLE_KEY, target.title);
  notifyFocusTargetChanged();
}

export function clearFocusTarget() {
  localStorage.removeItem(FOCUS_TARGET_UUID_KEY);
  localStorage.removeItem(FOCUS_TARGET_TITLE_KEY);
  notifyFocusTargetChanged();
}

function notifyFocusSettingsChanged() {
  window.dispatchEvent(new Event(FOCUS_SETTINGS_CHANGED_EVENT));
}

function notifyFocusTargetChanged() {
  window.dispatchEvent(new Event(FOCUS_TARGET_CHANGED_EVENT));
}

function readDurationMinutes(
  key: string,
  fallback: number,
  options: readonly number[],
) {
  return normalizeDurationMinutes(
    Number(readPreference(key)),
    fallback,
    options,
  );
}

function normalizeDurationMinutes(
  minutes: number,
  fallback: number,
  options: readonly number[],
) {
  return options.includes(minutes) ? minutes : fallback;
}
