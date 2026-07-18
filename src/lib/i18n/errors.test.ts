import { describe, expect, it } from "vitest";

import { setLanguageMode } from "$lib/i18n";
import { ensureErrorCode, localizedErrorMessage } from "./errors";

describe("localizedErrorMessage", () => {
  it("renders stable error codes in the selected language", () => {
    const coded = ensureErrorCode("timeout", "SYNC_NETWORK");
    setLanguageMode("en-US");
    expect(localizedErrorMessage(coded)).toContain("sync service");
    setLanguageMode("zh-CN");
    expect(localizedErrorMessage(coded)).toContain("同步服务");
  });

  it("keeps legacy string errors as a short safe fallback", () => {
    const message = localizedErrorMessage("旧版错误\nstack trace\nmore details");
    expect(message).toBe("旧版错误");
  });
});
