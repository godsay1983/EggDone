import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import fixtures from "../../../docs/fixtures/action-controls-v1.json";
const require = createRequire(import.meta.url);
const postcss = createRequire(require.resolve("vite"))("postcss");
const css = readFileSync("src/lib/styles/actions.css", "utf8");
const sheets = postcss.parse(css);
function contrast(a: string, b: string): number {
  const luminance = (hex: string) => {
    const c = hex.slice(1).match(/../g)!.map(v => parseInt(v, 16) / 255)
      .map(v => v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
    return c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
  };
  const [x, y] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (x + 0.05) / (y + 0.05);
}
describe("action control tokens", () => {
  for (const [theme, roles] of Object.entries(fixtures)) {
    const selector = theme === "light" ? ":root" : 'html[data-theme="dark"]';
    const values: Record<string, string> = {};
    sheets.walkRules(selector, (rule: { walkDecls: (callback: (d: { prop: string; value: string }) => void) => void }) => {
      rule.walkDecls(d => { values[d.prop] = d.value; });
    });
    for (const [role, colors] of Object.entries(roles)) {
      it(theme + " " + role + " uses shared colors with readable text", () => {
        const prefix = "--action" + (role === "normal" ? "" : "-" + role);
        expect(values[prefix + "-bg"]).toBe(colors.background);
        expect(values[prefix + "-text"]).toBe(colors.text);
        expect(contrast(colors.background, colors.text)).toBeGreaterThanOrEqual(4.5);
      });
    }
  }
  it("keeps wrap, keyboard focus and coarse-pointer targets", () => {
    expect(css).toContain("white-space: normal");
    expect(css).toContain(":focus-visible");
    expect(css).toContain("(pointer: coarse)");
    expect(css).toContain("min-height: 44px");
    expect(css).toContain(".action-button:disabled");
    expect(css).toContain('[aria-busy="true"]');
  });
});
