import { describe, expect, it } from "vitest";

import { formatFileSize, formatRelativeTime } from "./formatters";
import {
  normalizeLanguageMode,
  resolveSystemLanguage,
  translate,
  type TranslationKey,
} from "./index";
import { enUS } from "./locales/en-US";
import { zhCN } from "./locales/zh-CN";

describe("desktop i18n foundation", () => {
  it("keeps Chinese and English catalog keys aligned", () => {
    expect(Object.keys(enUS).sort()).toEqual(Object.keys(zhCN).sort());
  });

  it("normalizes persisted language modes", () => {
    expect(normalizeLanguageMode("zh-CN")).toBe("zh-CN");
    expect(normalizeLanguageMode("en-US")).toBe("en-US");
    expect(normalizeLanguageMode("system")).toBe("system");
    expect(normalizeLanguageMode("fr-FR")).toBe("system");
    expect(normalizeLanguageMode(null)).toBe("system");
  });

  it("resolves Chinese system tags and falls back to English", () => {
    expect(resolveSystemLanguage(["zh-Hans-CN"])).toBe("zh-CN");
    expect(resolveSystemLanguage(["zh-TW", "en-US"])).toBe("zh-CN");
    expect(resolveSystemLanguage(["en-GB"])).toBe("en-US");
    expect(resolveSystemLanguage([])).toBe("en-US");
  });

  it("interpolates parameters and applies English plurals", () => {
    expect(translate("zh-CN", "focus.currentTarget", { title: "写报告" })).toBe(
      "正在专注：写报告",
    );
    expect(translate("en-US", "todo.remainingCount", { count: 1 })).toBe(
      "1 task left",
    );
    expect(translate("en-US", "todo.remainingCount", { count: 3 })).toBe(
      "3 tasks left",
    );
  });

  it("falls back to the semantic key when a catalog entry is missing", () => {
    const missingKey = "test.missing" as TranslationKey;
    expect(translate("en-US", missingKey)).toBe(missingKey);
  });

  it("formats relative time and binary file sizes", () => {
    expect(formatFileSize(1536, "en-US")).toBe("1.5 KiB");
    expect(formatRelativeTime(0, 60_000, "en-US")).toBe("1 minute ago");
  });
});
