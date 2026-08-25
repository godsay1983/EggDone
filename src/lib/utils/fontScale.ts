import type { TranslationKey } from "$lib/i18n";

export type FontScaleLevel = "standard" | "large" | "xlarge";

export const FONT_SCALE_KEY = "eggdone-font-scale";

export const fontScaleOptions: Array<{
  value: FontScaleLevel;
  label: TranslationKey;
  scale: number;
}> = [
  { value: "standard", label: "settings.fontSizeStandard", scale: 1 },
  { value: "large", label: "settings.fontSizeLarge", scale: 1.15 },
  { value: "xlarge", label: "settings.fontSizeXLarge", scale: 1.3 },
];

export function normalizeFontScale(value: string | null): FontScaleLevel {
  return value === "large" || value === "xlarge" ? value : "standard";
}

export function getFontScale(): FontScaleLevel {
  return normalizeFontScale(localStorage.getItem(FONT_SCALE_KEY));
}

export function applyFontScale(level: FontScaleLevel) {
  const scale =
    fontScaleOptions.find((option) => option.value === level)?.scale ?? 1;
  document.documentElement.style.setProperty("--font-scale", String(scale));
}

export function saveFontScale(level: FontScaleLevel): FontScaleLevel {
  localStorage.setItem(FONT_SCALE_KEY, level);
  applyFontScale(level);
  return level;
}
