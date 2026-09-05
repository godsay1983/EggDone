import { describe, expect, it } from "vitest";
import { captureContent, captureTitle, normalizeCapture } from "./capture";

const input = { target: "note", title: "", body: "", source_url: "", source_app: "" };
describe("pending capture", () => {
  it("keeps an empty draft empty and does not create data", () => {
    const draft = normalizeCapture(input);
    expect(captureTitle(draft)).toBe("");
    expect(captureContent(draft)).toBe("");
  });
  it("preserves line breaks, Chinese and English but removes controls", () => {
    const draft = normalizeCapture({ ...input, title: "  学习\nEnglish  ", body: "a\r\nb\u0000\u202Ec" });
    expect(draft.title).toBe("学习 English");
    expect(draft.body).toBe("a\nbc");
  });
  it("limits text without splitting emoji", () => {
    const draft = normalizeCapture({ ...input, title: "x".repeat(99) + "😀", body: "y".repeat(20001) });
    expect(draft.title.length).toBe(99);
    expect(draft.body.length).toBe(20000);
    expect(draft.truncated).toBe(true);
  });
  it("keeps source URL once and derives only a fallback title", () => {
    const draft = normalizeCapture({ ...input, body: "Read this", source_url: "https://example.com" });
    expect(captureTitle(draft)).toBe("Read this");
    expect(captureContent(draft)).toBe("Read this\n\nhttps://example.com");
    expect(captureContent({ ...draft, body: captureContent(draft) })).toBe(captureContent(draft));
  });
  it.each(["file:///C:/private", "javascript:alert(1)", "data:text/plain,hi", "https://u:p@example.com",
    "https://example.com/\u0000bad", "https://example.com/\\bad", "https://example.com/white space"])("rejects unsafe source %s", (source_url) => {
    expect(() => normalizeCapture({ ...input, source_url })).toThrow();
  });
  it("rejects malformed target and oversized envelope", () => {
    expect(() => normalizeCapture({ ...input, target: "file" })).toThrow();
    expect(() => normalizeCapture({ ...input, body: "x".repeat(100001) })).toThrow();
  });
});
