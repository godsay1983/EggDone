import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || "playwright");
const root = path.resolve(import.meta.dirname, "..");
const fixtures = JSON.parse(fs.readFileSync(path.join(root, "docs/fixtures/action-controls-v1.json"), "utf8"));
const css = fs.readFileSync(path.join(root, "src/lib/styles/actions.css"), "utf8") + "\n" +
  fs.readFileSync(path.join(root, "src/app.css"), "utf8").replace('@import "./lib/styles/actions.css";', "");
const browser = await chromium.launch({ headless: true, channel: process.env.BROWSER_CHANNEL });
let checks = 0;
try {
  for (const width of [280, 380, 800]) for (const theme of ["light", "dark"])
    for (const zoom of [1, 1.5]) for (const locale of ["zh", "en"]) {
      const page = await browser.newPage({ viewport: { width, height: 1000 } });
      const [back, save, del, disabled] = locale === "en" ? ["Back", "Save changes", "Delete", "Unavailable"] :
        ["返回", "保存修改", "删除", "暂不可用"];
      const buttons = '<button class="action-button">' + back + '</button>' +
        '<button class="action-button" data-tone="primary">' + save + '</button>' +
        '<button class="action-button" data-tone="danger">' + del + '</button>' +
        '<button class="action-button" disabled>' + disabled + '</button>';
      await page.setContent('<html data-theme="' + theme + '"><style>' + css +
        '\nbody{height:auto;overflow:auto;padding:8px} main{width:100%} .probe{position:static;inset:auto;display:block;height:auto;min-height:0;margin:0 0 12px;width:100%}' +
        '</style><body><main><article class="note-card probe"><div class="note-card-actions">' + buttons +
        '</div></article><section class="note-editor probe"><header>' + buttons +
        '</header><footer>' + buttons + '</footer></section><div class="schedule-popover probe"><div class="schedule-footer">' +
        buttons + '</div></div><div class="note-popover probe"><div class="note-footer"><div>' + buttons +
        '</div></div></div></main></body></html>');
      await page.evaluate(z => { document.documentElement.style.zoom = String(z); }, zoom);
      const problems = await page.locator(".action-button").evaluateAll(buttons => buttons.flatMap(button => {
        const rect = button.getBoundingClientRect(), style = getComputedStyle(button);
        const issues = [];
        if (rect.right > innerWidth + 1 || rect.left < -1) issues.push("outside viewport");
        if (button.scrollWidth > button.clientWidth + 1 || button.scrollHeight > button.clientHeight + 1) issues.push("clipped");
        if (parseFloat(style.fontSize) < 12 || parseFloat(style.minHeight) < 32) issues.push("small control");
        return issues.map(issue => button.textContent + ": " + issue);
      }));
      assert.deepEqual(problems, [], JSON.stringify({width, theme, zoom, locale}));
      const rendered = await page.locator(".action-button").evaluateAll(buttons => buttons.map(button => {
        const style = getComputedStyle(button), r = button.getBoundingClientRect();
        return { role: button.disabled ? "disabled" : button.dataset.tone || "normal",
          foreground: style.color, background: style.backgroundColor,
          left: r.left, right: r.right, top: r.top, bottom: r.bottom };
      }));
      const rgb = hex => "rgb(" + hex.slice(1).match(/../g).map(v => parseInt(v, 16)).join(", ") + ")";
      for (const item of rendered) {
        assert.equal(item.foreground, rgb(fixtures[theme][item.role].text), theme + ":" + item.role);
        assert.equal(item.background, rgb(fixtures[theme][item.role].background), theme + ":" + item.role);
      }
      for (let i = 0; i < rendered.length; i++) for (let j = i + 1; j < rendered.length; j++) {
        const a = rendered[i], b = rendered[j];
        assert.ok(Math.min(a.right, b.right) - Math.max(a.left, b.left) <= 1 ||
          Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) <= 1, "overlapping controls");
      }
      await page.locator(".action-button").first().focus();
      assert.equal(await page.locator(".action-button").first().evaluate(el => getComputedStyle(el).outlineStyle), "solid");
      const clickCount = await page.evaluate(() => {
        let count = 0;
        const button = document.querySelector("button:disabled");
        button.addEventListener("click", () => count++);
        button.click();
        return count;
      });
      assert.equal(clickCount, 0);
      if (width === 380 && theme === "dark" && zoom === 1.5 && locale === "en") {
        const output = path.join(os.tmpdir(), "eggdone-action-controls.png");
        await page.screenshot({ path: output, fullPage: true });
        console.log("Screenshot: " + output);
      }
      checks++;
      await page.close();
    }
  console.log(checks + " production-CSS layout combinations passed (not native app acceptance).");
} finally { await browser.close(); }
