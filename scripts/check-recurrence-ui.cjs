// Render the real Svelte component with mocked Tauri transport in an isolated browser.
const assert = require('node:assert/strict');
const path = require('node:path');
const fs = require('node:fs');
const { chromium } = require(process.env.PLAYWRIGHT_PATH || 'playwright');
const base = process.env.EGGDONE_TEST_URL || 'http://127.0.0.1:1423';
const output = process.env.EGGDONE_UI_OUTPUT || path.join(require('node:os').tmpdir(), 'eggdone-recurrence-ui');
const todo = { id: 1, uuid: '00000000-0000-4000-8000-000000000001', title: 'Review a long task title with a recurring schedule',
  note: null, group_uuid: null, completed: false, pinned: false, priority: 0, sort_order: 0, created_at: 1, updated_at: 1,
  completed_at: null, deleted_at: null, archived_at: null, due_date: '2026-09-06', due_at: null, reminder_at: null,
  repeat_rule: null, repeat_next_due_date: null, repeat_series_uuid: null };
async function mount(page, locale, theme, mode = 'create') {
  page.on('pageerror', error => console.error('Browser error:', error.stack));
  page.on('console', message => { if (message.type() === 'error') console.error('Browser console:', message.text()); });
  await page.route('**/__recurrence_ui', route => route.fulfill({ contentType: 'text/html', body: '<!doctype html><html><head></head><body><div id="test"></div></body></html>' }));
  await page.goto(base + '/__recurrence_ui');
  await page.evaluate(({ todo, locale, theme, mode }) => {
    document.documentElement.dataset.theme = theme;
    window.attempts = []; window.closedEditor = false; window.stoppedRule = false;
    const rule = { uuid: '00000000-0000-4000-8000-000000000002', first_todo_uuid: todo.uuid, current_todo_uuid: todo.uuid,
      schedule: { anchor_date: '2026-09-06', frequency: 'daily', interval: 1, weekdays: [], month_day: null,
        end_type: 'never', end_date: null, max_occurrences: null, local_time_minutes: null },
      timezone_id: null, current_date: '2026-09-06', generated_count: 1, exhausted: false, updated_at: 1,
      updated_by: '00000000-0000-4000-8000-000000000003', deleted_at: mode === 'stopped' ? 2 : null };
    window.caseTodo = { ...todo, repeat_rule: mode === 'legacy' ? 'daily' : null, repeat_series_uuid: mode === 'create' || mode === 'item' ? null : todo.uuid };
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      if (command === 'recurrence_editor_context') return { rules: mode === 'edit' || mode === 'stopped' ? [rule] : [], device_id: rule.updated_by };
      if (command === 'save_recurrence_rule') {
        window.attempts.push(structuredClone(args.request));
        if (window.attempts.length === 1) throw 'RECURRENCE_DATABASE: retry';
        return args.request.rule;
      }
      if (command === 'stop_recurrence_rule') { window.stoppedRule = true; return { ...args.expected, deleted_at: 2 }; }
      if (command === 'list_todos') return [todo];
      if (command === 'list_groups') return [];
      return null;
    }};
    window.caseLocale = locale;
  }, { todo, locale, theme, mode });
  const transformed = await (await page.request.get(base + '/src/lib/components/RecurrenceEditor.svelte')).text();
  const runtime = transformed.match(/from "([^"]*\/svelte\.js[^"]*)"/)[1];
  await page.addScriptTag({ type: 'module', content: `
    import '/src/app.css';
    import { mount } from '${runtime}';
    import Editor from '/src/lib/components/${mode === 'item' ? 'TodoItem' : 'RecurrenceEditor'}.svelte';
    import { setLanguageMode } from '/src/lib/i18n/index.ts';
    setLanguageMode(window.caseLocale);
    mount(Editor, { target: document.getElementById('test'), props: { todo: window.caseTodo, onClose: () => window.closedEditor = true } });
  ` });
  if (mode === 'item') { await page.locator('article').waitFor(); return; }
  await page.locator('dialog[open]').waitFor();
  await page.waitForFunction(() => !document.querySelector('button.close')?.disabled);
}
async function main() {
  fs.mkdirSync(output, { recursive: true });
  const browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_CHANNEL || 'chrome' });
  let count = 0;
  try {
    for (const [width, height] of [[320,480],[480,720],[920,560]]) for (const locale of ['en-US','zh-CN']) for (const theme of ['light','dark']) {
      const page = await browser.newPage({ viewport: { width, height } });
      await mount(page, locale, theme);
      const geometry = await page.evaluate(() => {
        const d = document.querySelector('dialog'); const footer = d.querySelector('footer').getBoundingClientRect();
        const fields = d.querySelector('.fields'); const bounds = d.getBoundingClientRect();
        return { fits: bounds.x >= 0 && bounds.right <= innerWidth && bounds.y >= 0 && bounds.bottom <= innerHeight,
          footerVisible: footer.bottom <= innerHeight, overflow: fields.scrollWidth > fields.clientWidth + 1,
          scrolls: fields.scrollHeight > fields.clientHeight };
      });
      assert.equal(geometry.fits, true); assert.equal(geometry.footerVisible, true); assert.equal(geometry.overflow, false);
      if (height === 480) assert.equal(geometry.scrolls, true);
      await page.screenshot({ path: path.join(output, width + '-' + locale + '-' + theme + '.png') });
      count++; await page.close();
    }
    const page = await browser.newPage({ viewport: { width: 480, height: 720 } });
    await mount(page, 'en-US', 'dark');
    await page.getByRole('combobox', { name: /^Frequency/ }).selectOption('weekly');
    await page.getByRole('button', { name: 'Mon', exact: true }).click();
    await page.getByRole('button', { name: 'Sun', exact: true }).click();
    await page.getByRole('combobox', { name: /^Ends/ }).selectOption('count');
    await page.getByLabel('Occurrences (1–100000)', { exact: true }).fill('2');
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.getByRole('alert').waitFor();
    await page.getByRole('button', { name: 'Save', exact: true }).click();
    await page.waitForFunction(() => window.closedEditor);
    const attempts = await page.evaluate(() => window.attempts);
    assert.equal(attempts.length, 2); assert.deepEqual(attempts[0], attempts[1]);
    assert.equal(attempts[0].rule.schedule.anchor_date, '2026-09-07');
    assert.deepEqual(attempts[0].rule.schedule.weekdays, [1]);
    assert.equal(attempts[0].rule.schedule.max_occurrences, 2);
    count++; await page.close();
    for (const mode of ['legacy', 'stopped']) {
      const page = await browser.newPage();
      await mount(page, 'en-US', 'light', mode);
      assert.equal(await page.getByRole('button', { name: 'Save', exact: true }).count(), 0);
      assert.ok(await page.getByText('This task cannot be converted directly.', { exact: false }).isVisible());
      count++; await page.close();
    }
    const stop = await browser.newPage();
    await mount(stop, 'en-US', 'light', 'edit');
    stop.once('dialog', d => d.accept());
    await stop.getByRole('button', { name: 'Stop repeating', exact: true }).click();
    await stop.waitForFunction(() => window.stoppedRule && window.closedEditor);
    count++; await stop.close();
    const item = await browser.newPage({ viewport: { width: 480, height: 720 } });
    await mount(item, 'en-US', 'light', 'item');
    await item.locator('.due-badge').click();
    await item.locator('.schedule-popover input[type="date"]').fill('2026-09-10');
    await item.getByRole('button', { name: 'Custom repeat', exact: true }).click();
    assert.equal(await item.locator('dialog[open]').count(), 0);
    assert.equal(await item.locator('.schedule-popover input[type="date"]').inputValue(), '2026-09-10');
    assert.ok(await item.getByText('Save current task edits before setting custom repeat.', { exact: true }).isVisible());
    count++; await item.close();
    console.log(count + ' browser scenarios passed. Screenshots: ' + output);
  } finally { await browser.close(); }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
