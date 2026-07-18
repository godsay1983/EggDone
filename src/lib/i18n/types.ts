export type ResolvedLocale = "zh-CN" | "en-US";

export type LanguageMode = "system" | ResolvedLocale;

export interface LanguageState {
  mode: LanguageMode;
  resolvedLocale: ResolvedLocale;
}

export interface PluralMessage {
  one: string;
  other: string;
}

export type TranslationMessage = string | PluralMessage;

export type TranslationParams = Record<string, string | number>;

