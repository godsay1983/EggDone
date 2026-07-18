import { derived, get, writable } from "svelte/store";

import { enUS } from "./locales/en-US";
import { zhCN } from "./locales/zh-CN";
import type {
  LanguageMode,
  LanguageState,
  ResolvedLocale,
  TranslationMessage,
  TranslationParams,
} from "./types";

export type TranslationKey = keyof typeof zhCN;
export type { LanguageMode, LanguageState, ResolvedLocale, TranslationParams };

export const LANGUAGE_STORAGE_KEY = "eggdone-language";

const DEFAULT_STATE: LanguageState = {
  mode: "system",
  resolvedLocale: "en-US",
};

export const languageState = writable<LanguageState>(DEFAULT_STATE);

export const translator = derived(
  languageState,
  ($languageState) =>
    (key: TranslationKey, params: TranslationParams = {}) =>
      translate($languageState.resolvedLocale, key, params),
);

let initialized = false;

export function initializeLanguage(): LanguageState {
  if (typeof window === "undefined") return get(languageState);
  if (!initialized) {
    initialized = true;
    window.addEventListener("storage", handleStorageChange);
    window.addEventListener("languagechange", handleSystemLanguageChange);
  }
  return applyLanguageMode(
    normalizeLanguageMode(window.localStorage.getItem(LANGUAGE_STORAGE_KEY)),
  );
}

export function setLanguageMode(mode: LanguageMode): LanguageState {
  const normalized = normalizeLanguageMode(mode);
  if (typeof window !== "undefined") {
    window.localStorage.setItem(LANGUAGE_STORAGE_KEY, normalized);
  }
  return applyLanguageMode(normalized);
}

export function getLanguageState(): LanguageState {
  return get(languageState);
}

export function normalizeLanguageMode(value: string | null | undefined): LanguageMode {
  if (value === "zh-CN" || value === "en-US" || value === "system") {
    return value;
  }
  return "system";
}

export function resolveSystemLanguage(
  languages: readonly string[] = systemLanguages(),
): ResolvedLocale {
  const language = languages.find((candidate) => candidate.trim().length > 0);
  return language?.toLowerCase().startsWith("zh") ? "zh-CN" : "en-US";
}

export function translate(
  locale: ResolvedLocale,
  key: TranslationKey,
  params: TranslationParams = {},
): string {
  const catalog: Record<TranslationKey, TranslationMessage> =
    locale === "zh-CN" ? zhCN : enUS;
  const fallbackCatalog: Record<TranslationKey, TranslationMessage> = enUS;
  const message = catalog[key] ?? fallbackCatalog[key];
  if (message === undefined) return key;
  return interpolate(selectMessage(locale, message, params), params);
}

function applyLanguageMode(mode: LanguageMode): LanguageState {
  const next: LanguageState = {
    mode,
    resolvedLocale: mode === "system" ? resolveSystemLanguage() : mode,
  };
  languageState.set(next);
  if (typeof document !== "undefined") {
    document.documentElement.lang = next.resolvedLocale;
  }
  return next;
}

function selectMessage(
  locale: ResolvedLocale,
  message: TranslationMessage,
  params: TranslationParams,
): string {
  if (typeof message === "string") return message;
  const count = Number(params.count ?? 0);
  return new Intl.PluralRules(locale).select(count) === "one"
    ? message.one
    : message.other;
}

function interpolate(template: string, params: TranslationParams): string {
  return template.replace(/\{([A-Za-z][A-Za-z0-9_]*)\}/g, (match, key) =>
    Object.prototype.hasOwnProperty.call(params, key) ? String(params[key]) : match,
  );
}

function systemLanguages(): readonly string[] {
  if (typeof navigator === "undefined") return [];
  if (navigator.languages.length > 0) return navigator.languages;
  return navigator.language ? [navigator.language] : [];
}

function handleStorageChange(event: StorageEvent) {
  if (event.key === null || event.key === LANGUAGE_STORAGE_KEY) {
    applyLanguageMode(
      normalizeLanguageMode(window.localStorage.getItem(LANGUAGE_STORAGE_KEY)),
    );
  }
}

function handleSystemLanguageChange() {
  const current = get(languageState);
  if (current.mode === "system") applyLanguageMode("system");
}

