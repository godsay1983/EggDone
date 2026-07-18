import { getLanguageState, type ResolvedLocale } from "./index";

export function formatDate(
  value: Date | number,
  options: Intl.DateTimeFormatOptions = { year: "numeric", month: "short", day: "numeric" },
  locale: ResolvedLocale = getLanguageState().resolvedLocale,
): string {
  return new Intl.DateTimeFormat(locale, options).format(toDate(value));
}

export function formatTime(
  value: Date | number,
  locale: ResolvedLocale = getLanguageState().resolvedLocale,
): string {
  return new Intl.DateTimeFormat(locale, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(toDate(value));
}

export function formatDateTime(
  value: Date | number,
  locale: ResolvedLocale = getLanguageState().resolvedLocale,
): string {
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(toDate(value));
}

export function formatRelativeTime(
  value: Date | number,
  now = Date.now(),
  locale: ResolvedLocale = getLanguageState().resolvedLocale,
): string {
  const differenceSeconds = Math.round((toDate(value).getTime() - now) / 1000);
  const absoluteSeconds = Math.abs(differenceSeconds);
  if (absoluteSeconds < 60) return relative(locale, differenceSeconds, "second");
  const differenceMinutes = Math.round(differenceSeconds / 60);
  if (Math.abs(differenceMinutes) < 60) return relative(locale, differenceMinutes, "minute");
  const differenceHours = Math.round(differenceMinutes / 60);
  if (Math.abs(differenceHours) < 24) return relative(locale, differenceHours, "hour");
  return relative(locale, Math.round(differenceHours / 24), "day");
}

export function formatNumber(
  value: number,
  locale: ResolvedLocale = getLanguageState().resolvedLocale,
): string {
  return new Intl.NumberFormat(locale).format(value);
}

export function formatFileSize(
  bytes: number,
  locale: ResolvedLocale = getLanguageState().resolvedLocale,
): string {
  const safeBytes = Number.isFinite(bytes) && bytes > 0 ? bytes : 0;
  const units = ["B", "KiB", "MiB", "GiB"] as const;
  let value = safeBytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  const number = new Intl.NumberFormat(locale, {
    minimumFractionDigits: 0,
    maximumFractionDigits: unitIndex === 0 ? 0 : 1,
  }).format(value);
  return `${number} ${units[unitIndex]}`;
}

function relative(
  locale: ResolvedLocale,
  value: number,
  unit: Intl.RelativeTimeFormatUnit,
): string {
  return new Intl.RelativeTimeFormat(locale, { numeric: "auto" }).format(value, unit);
}

function toDate(value: Date | number): Date {
  return value instanceof Date ? value : new Date(value);
}

