// Optional browser check. PLAYWRIGHT_PATH may point to a bundled Playwright package.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_PATH || "playwright");
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const html = `<!doctype html><html><head><meta charset="UTF-8"></head><body>
<script type="module">
import { mount, unmount } from 'svelte';
import CaptureDialog from '/src/lib/components/CaptureDialog.svelte';
import { setLanguageMode } from '/src/lib/i18n/index.ts';
import '/src/app.css';
const options = new URLSearchParams(location.search);
setLanguageMode(options.get('lang') || 'en-US');
document.documentElement.dataset.theme = options.get('theme') || 'light';
window.saved = [];
window.cancelled = 0;
const component = mount(CaptureDialog, { target: document.body, props: {
  draft: { target: 'note', title: '', body: '', source_url: '', source_app: '', truncated: false },
  onSave: async (draft) => { window.saved.push(draft); await unmount(component); },
  onCancel: () => { window.cancelled++; void unmount(component); }
}});
</script></body></html>`;
const server = await createServer({
  root, configFile: false,
  resolve: { alias: { $lib: resolve(root, 'src/lib') }, conditions: ['browser'] },
  plugins: [svelte(), { name: 'capture-check', configureServer(server) {
    server.middlewares.use('/__capture-test', async (_request, response) => {
      response.setHeader('Content-Type', 'text/html; charset=utf-8');
      response.end(await server.transformIndexHtml('/__capture-test', html));
    });
  } }],
  optimizeDeps: { noDiscovery: true, include: ['svelte', 'svelte/store'] },
  server: { host: '127.0.0.1', port: 5187, watch: { ignored: ['**/src-tauri/**'] } },
});
let browser;
try {
  await server.listen();
  const base = server.resolvedUrls.local[0];
  browser = await chromium.launch({ headless: true, channel: 'msedge' });
  const page = await browser.newPage();
  page.setDefaultTimeout(15000);
  page.setDefaultNavigationTimeout(15000);
  const errors = [];
  page.on('pageerror', (error) => { errors.push(error.message); console.error('Browser:', error.message); });
  for (const language of ['en-US', 'zh-CN']) {
    for (const theme of ['light', 'dark']) {
      for (const size of [{ width: 320, height: 430 }, { width: 520, height: 720 }]) {
        console.log('Checking capture UI', language, theme, size.width, size.height);
        await page.setViewportSize(size);
        await page.goto(base + '__capture-test?lang=' + language + '&theme=' + theme);
        await page.locator('dialog[open]').waitFor();
        assert.equal(await page.evaluate(() => window.saved.length), 0);
        const buttons = page.locator('dialog footer button');
        for (let i = 0; i < 2; i++) {
          const bounds = await buttons.nth(i).boundingBox();
          assert.ok(bounds.x >= 0 && bounds.y >= 0 && bounds.x + bounds.width <= size.width &&
            bounds.y + bounds.height <= size.height, 'footer must stay inside viewport');
        }
        assert.equal(await page.locator('dialog').evaluate((node) => node.scrollWidth <= node.clientWidth), true);
        assert.equal(await page.locator('.capture-fields').evaluate((node) => node.scrollWidth <= node.clientWidth), true);
        await page.locator('dialog input:not([type="checkbox"])').fill('Test title');
        await page.locator('dialog textarea').fill('Shared text');
        if (process.env.CAPTURE_SCREENSHOT_DIR && language === 'en-US' && size.width === 320) {
          await page.screenshot({ path: resolve(process.env.CAPTURE_SCREENSHOT_DIR, 'eggdone-capture-' + theme + '.png') });
        }
        await buttons.nth(1).click();
        await page.waitForFunction(() => window.saved.length === 1);
        assert.equal(await page.evaluate(() => window.saved[0].body), 'Shared text');
        await page.goto(base + '__capture-test?lang=' + language + '&theme=' + theme);
        await page.locator('dialog[open]').waitFor();
        await page.locator('dialog textarea').fill('cancel this');
        await page.keyboard.press('Escape');
        assert.equal(await page.evaluate(() => window.saved.length), 0);
        assert.equal(await page.evaluate(() => window.cancelled), 1);
      }
    }
  }
  assert.deepEqual(errors, []);
  console.log('Capture UI: 8 language/theme/window combinations passed; cancel and confirm checked.');
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  await browser?.close();
  await server.close();
}
