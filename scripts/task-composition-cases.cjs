const assert = require('node:assert/strict');

module.exports = function createCases(api, fixtures) {
  const clone = value => JSON.parse(JSON.stringify(value));
  const draft = () => api.copyTaskForCreation(clone(fixtures.source), 'draft-a');
  const cases = fixtures.parses.map(c => ({
    name: 'parse: ' + c.id,
    run: () => {
      const result = api.parseBatchTaskText(c.input);
      assert.equal(result.error, null);
      assert.deepEqual(result.rows.map(r => r.title), c.titles);
      assert.deepEqual(result.rows.map(r => r.lineNumber), c.lines);
      assert.deepEqual(result.rows.map(r => r.duplicate), c.duplicates);
      assert.deepEqual(result.rows.map(r => r.issue), c.issues);
      assert.ok(result.rows.every(r => r.selected));
    }
  }));
  function test(name, run) { cases.push({ name, run }); }

  test('copy whitelists content, resets state and owns its arrays', () => {
    const source = clone(fixtures.source), before = clone(source);
    const copy = api.copyTaskForCreation(source, 'copy-a');
    assert.deepEqual(copy, {
      draftKey: 'copy-a', title: '发布版本', note: '测试通过\n上传安装包', groupUuid: source.groupUuid,
      checklist: [
        { rowKey: 'copy-a:item:0', content: '编译', completed: false },
        { rowKey: 'copy-a:item:1', content: '提交审核', completed: false }
      ],
      completed: false, isPinned: false, priority: 0, dueAt: null, reminderAt: null, repeatRule: null
    });
    assert.deepEqual(api.validateTaskCreationDraft(copy), []);
    copy.checklist[0].content = 'changed';
    assert.deepEqual(source, before);
    assert.notEqual(api.copyTaskForCreation(source, 'copy-b').checklist[0].rowKey, copy.checklist[0].rowKey);
  });
  test('title and note limits reject without truncation', () => {
    const d = draft();
    d.title = 'x'.repeat(100); d.note = 'n'.repeat(1000);
    assert.deepEqual(api.validateTaskCreationDraft(d), []);
    d.title += 'x'; d.note += 'n';
    assert.deepEqual(api.validateTaskCreationDraft(d).map(i => i.code), ['TITLE_TOO_LONG', 'NOTE_TOO_LONG']);
    assert.equal(d.title.length, 101);
    d.title = ' '; d.note = 'bad\u0000';
    assert.deepEqual(api.validateTaskCreationDraft(d).map(i => i.code), ['EMPTY_TITLE', 'INVALID_NOTE']);
  });
  test('UTF-16 limits preserve complete emoji and reject broken pairs', () => {
    const d = draft();
    d.title = '🥚'.repeat(50);
    assert.deepEqual(api.validateTaskCreationDraft(d), []);
    d.title += 'a';
    assert.equal(api.validateTaskCreationDraft(d)[0].code, 'TITLE_TOO_LONG');
    for (const text of ['x\uD800', 'x\uDC00', 'x\uD800y', 'a\nb', 'a\tb', 'a\u2028b', 'a\u2066b']) {
      d.title = text;
      assert.equal(api.validateTaskCreationDraft(d)[0].code, 'INVALID_TITLE');
    }
  });
  test('checklist limits and per-row validation', () => {
    const d = draft();
    d.checklist = Array.from({ length: 20 }, (_, i) => ({ rowKey: 'r' + i, content: 'x'.repeat(200), completed: false }));
    assert.deepEqual(api.validateTaskCreationDraft(d), []);
    d.checklist.push({ rowKey: 'r20', content: 'x', completed: false });
    assert.equal(api.validateTaskCreationDraft(d)[0].code, 'TOO_MANY_ITEMS');
    d.checklist = [
      { rowKey: 'r1', content: '', completed: false },
      { rowKey: 'r1', content: 'x'.repeat(201), completed: false },
      { rowKey: 'bad/key', content: 'a\nb', completed: false }
    ];
    assert.deepEqual(api.validateTaskCreationDraft(d), [
      { code: 'EMPTY_ITEM', index: 0 }, { code: 'INVALID_ITEM_KEY', index: 1 },
      { code: 'ITEM_TOO_LONG', index: 1 }, { code: 'INVALID_ITEM_KEY', index: 2 },
      { code: 'INVALID_ITEM', index: 2 }
    ]);
  });
  test('template is a detached content snapshot, not a link', () => {
    const source = clone(fixtures.source);
    const snapshot = api.makeTaskTemplateSnapshot(source, ' 发布 ');
    assert.deepEqual(snapshot, { name: '发布', title: '发布版本', note: '测试通过\n上传安装包',
      groupUuid: source.groupUuid, checklist: ['编译', '提交审核'] });
    source.checklist[0].content = 'source changed';
    const one = api.taskTemplateToDraft(snapshot, 'one'), two = api.taskTemplateToDraft(snapshot, 'two');
    one.checklist[0].content = 'draft changed';
    assert.equal(two.checklist[0].content, '编译');
    assert.equal(snapshot.checklist[0], '编译');
    assert.equal(two.dueAt, null);
    assert.equal(two.reminderAt, null);
    assert.equal(two.repeatRule, null);
  });
  test('invalid templates cannot be saved', () => {
    for (const [name, error] of [['', 'EMPTY_TEMPLATE_NAME'], ['x'.repeat(61), 'TEMPLATE_NAME_TOO_LONG'],
      ['a\nb', 'INVALID_TEMPLATE_NAME']]) {
      assert.throws(() => api.makeTaskTemplateSnapshot(fixtures.source, name), new RegExp(error));
    }
    assert.throws(() => api.makeTaskTemplateSnapshot({ ...fixtures.source, title: '' }, 'valid'), /EMPTY_TITLE/);
    const d = api.makeTaskTemplateSnapshot(fixtures.source, 'x'.repeat(60));
    assert.equal(d.name.length, 60);
  });
  test('batch limits reject the entire input rather than silently keeping a prefix', () => {
    assert.equal(api.parseBatchTaskText('x'.repeat(20000)).error, null);
    assert.deepEqual(api.parseBatchTaskText('x'.repeat(20001)), { rows: [], error: 'INPUT_TOO_LONG' });
    assert.equal(api.parseBatchTaskText(Array(50).fill('task').join('\n')).rows.length, 50);
    assert.deepEqual(api.parseBatchTaskText(Array(51).fill('task').join('\n')), { rows: [], error: 'TOO_MANY_TASKS' });
  });
  test('batch conversion preserves line identity and resets scheduling', () => {
    const preview = api.parseBatchTaskText('明天提交报告\n\n买牛奶\n买牛奶');
    const all = api.batchPreviewToDrafts(preview, 'request', null);
    assert.equal(all.length, 3);
    preview.rows[0].selected = false;
    const selected = api.batchPreviewToDrafts(preview, 'request', 'group');
    assert.deepEqual(selected.map(d => d.draftKey), all.slice(1).map(d => d.draftKey));
    assert.deepEqual(selected.map(d => d.draftKey), ['request:3', 'request:4']);
    assert.ok(selected.every(d => d.groupUuid === 'group' && d.dueAt === null &&
      d.reminderAt === null && d.repeatRule === null && d.completed === false));
    assert.deepEqual(api.batchPreviewToDrafts(preview, 'request', 'group'), selected);
  });
  test('batch confirmation revalidates edits and ignores deselected bad rows', () => {
    const preview = api.parseBatchTaskText('ok\n' + 'x'.repeat(101));
    assert.throws(() => api.batchPreviewToDrafts(preview, 'req', null), /TITLE_TOO_LONG/);
    preview.rows[1].selected = false;
    assert.equal(api.batchPreviewToDrafts(preview, 'req', null).length, 1);
    preview.rows[1].selected = true;
    preview.rows[1].title = 'fixed';
    assert.equal(api.batchPreviewToDrafts(preview, 'req', null).length, 2);
    preview.rows[0].title = '';
    assert.throws(() => api.batchPreviewToDrafts(preview, 'req', null), /EMPTY_TITLE/);
  });
  test('empty selection, invalid identities and duplicate line numbers are rejected', () => {
    assert.throws(() => api.batchPreviewToDrafts(api.parseBatchTaskText(''), 'req', null), /NO_TASKS_SELECTED/);
    const p = api.parseBatchTaskText('one\ntwo');
    p.rows.forEach(r => r.selected = false);
    assert.throws(() => api.batchPreviewToDrafts(p, 'req', null), /NO_TASKS_SELECTED/);
    p.rows.forEach(r => r.selected = true);
    p.rows[1].lineNumber = p.rows[0].lineNumber;
    assert.throws(() => api.batchPreviewToDrafts(p, 'req', null), /INVALID_BATCH_ROWS/);
    p.rows[1].lineNumber = 1.5;
    assert.throws(() => api.batchPreviewToDrafts(p, 'req', null), /INVALID_BATCH_ROWS/);
    assert.throws(() => api.batchPreviewToDrafts(api.parseBatchTaskText('task'), '../bad', null), /INVALID_DRAFT_KEY/);
    assert.throws(() => api.copyTaskForCreation(fixtures.source, ''), /INVALID_DRAFT_KEY/);
    assert.throws(() => api.copyTaskForCreation(fixtures.source, 'x'.repeat(101)), /INVALID_DRAFT_KEY/);
    assert.throws(() => api.batchPreviewToDrafts({ rows: [], error: 'INPUT_TOO_LONG' }, 'req', null), /INPUT_TOO_LONG/);
  });
  return cases;
};
