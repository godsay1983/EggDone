import { beforeEach, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { dataApi, type ImportPreview } from './dataApi';
import { translate } from '$lib/i18n';

beforeEach(() => { invoke.mockReset().mockResolvedValue(undefined); });

function preview(included: boolean, relations: number, completions: number): ImportPreview {
  return {
    path: 'backup.json', file_name: 'backup.json', total: 0, added: 0, updated: 0, unchanged: 0,
    note_total: 0, note_added: 0, note_updated: 0, note_unchanged: 0, attachment_total: 0,
    recurrence_total: 0, link_total: 0, link_deleted: 0, link_metadata_included: false,
    checklist_total: 0, checklist_deleted: 0, checklist_definition_total: 0,
    checklist_missing_parent_total: 0, checklist_metadata_included: false,
    template_total: 0, template_deleted: 0, template_metadata_included: false,
    planning_metadata_included: included, planning_relations: relations, planning_completions: completions,
    attachment_added: 0, attachment_updated: 0, attachment_unchanged: 0, attachment_files_included: false,
    backup_file_count: 0, backup_total_bytes: 0,
  };
}

it.each([
  ['json', 'included', true, 12, 3], ['backup', 'included', true, 12, 3],
  ['json', 'empty', true, 0, 0], ['backup', 'empty', true, 0, 0],
  ['json', 'legacy', false, 0, 0], ['backup', 'legacy', false, 0, 0],
] as const)('preserves %s %s planning disclosure without applying the import', async (kind, _state, included, relations, completions) => {
  const result = preview(included, relations, completions);
  invoke.mockResolvedValue(result);
  expect(await (kind === 'json' ? dataApi.previewImport() : dataApi.previewFullBackupImport())).toEqual(result);
  expect(invoke.mock.calls).toEqual([[kind === 'json' ? 'preview_todo_import' : 'preview_full_backup_import']]);
});

it('keeps confirm commands path-only, without sending preview counts or planning mutations', async () => {
  await dataApi.confirmImport('backup.json');
  await dataApi.confirmFullBackupImport('backup.zip');
  expect(invoke.mock.calls).toEqual([
    ['confirm_todo_import', { path: 'backup.json' }],
    ['confirm_full_backup_import', { path: 'backup.zip' }],
  ]);
});

it('keeps a cancelled native file selection as null', async () => {
  invoke.mockResolvedValue(null);
  expect(await dataApi.previewImport()).toBeNull();
  expect(await dataApi.previewFullBackupImport()).toBeNull();
});

it('distinguishes explicit zero counts from absent planning data in both locales', () => {
  expect(translate('zh-CN', 'data.planningSummary', { relations: 12, completions: 3 }))
    .toBe('今日计划关系 12 条，完成记录 3 条');
  expect(translate('zh-CN', 'data.planningSummary', { relations: 0, completions: 0 }))
    .toBe('今日计划关系 0 条，完成记录 0 条');
  expect(translate('zh-CN', 'data.planningLegacy')).toBe('不包含今日计划，保留当前安排');
  expect(translate('en-US', 'data.planningSummary', { relations: 12, completions: 3 }))
    .toBe('Daily plan relations: 12, completion records: 3');
  expect(translate('en-US', 'data.planningLegacy')).toBe('No daily planning data. Current plans are preserved.');
});
