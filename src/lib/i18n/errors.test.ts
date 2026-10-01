import { describe, expect, it } from "vitest";

import { setLanguageMode } from "$lib/i18n";
import { ensureErrorCode, localizedErrorMessage } from "./errors";

describe("localizedErrorMessage", () => {
  it("renders stable error codes in the selected language", async () => {
    const coded = ensureErrorCode("timeout", "SYNC_NETWORK");
    await setLanguageMode("en-US");
    expect(localizedErrorMessage(coded)).toContain("sync service");
    await setLanguageMode("zh-CN");
    expect(localizedErrorMessage(coded)).toContain("同步服务");
  });

  it("keeps legacy string errors as a short safe fallback", () => {
    const message = localizedErrorMessage("旧版错误\nstack trace\nmore details");
    expect(message).toBe("旧版错误");
  });

  it.each(['PROGRESS_INVALID', 'PROGRESS_LIMIT', 'PROGRESS_CONFLICT', 'PROGRESS_REMOTE_MISSING', 'PROGRESS_CONFIG_CHANGED',
    'PROGRESS_KEY_COLLISION', 'PROGRESS_DATABASE', 'PROGRESS_HTTP:403', 'PROGRESS_NETWORK', 'PROGRESS_FUTURE_CODE'])('localizes %s in notices and coded sync/import failures', async code => {
    for (const locale of ['en-US', 'zh-CN'] as const) {
      await setLanguageMode(locale);
      const raw = localizedErrorMessage(code), coded = localizedErrorMessage(ensureErrorCode(code, 'SYNC_FAILED'));
      expect(coded).toBe(raw); expect(raw).not.toContain('PROGRESS_'); expect(raw).not.toContain('taskProgress.');
      expect(raw.length).toBeGreaterThan(10);
      expect(localizedErrorMessage(ensureErrorCode(code, 'DATA_EXCHANGE_FAILED'))).toBe(raw);
    }
  });
});
