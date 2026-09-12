import { beforeEach, expect, it, vi } from 'vitest';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { contentSearchApi } from './contentSearchApi';
beforeEach(() => { invoke.mockReset(); });
it('requests bounded pages by type without altering literal input', async () => {
  invoke.mockResolvedValue({ items: [], total: 0 });
  await contentSearchApi.search('attachment', '100%_计划');
  expect(invoke).toHaveBeenLastCalledWith('search_content', { scope: 'attachment', query: '100%_计划', offset: 0, limit: 20 });
  await contentSearchApi.search('todo', 'report', 20, 20);
  expect(invoke).toHaveBeenLastCalledWith('search_content', { scope: 'todo', query: 'report', offset: 20, limit: 20 });
});
it('resolves by identity instead of trusting cached titles, body or file paths', async () => {
  invoke.mockResolvedValue({ title: 'Current title' });
  expect(await contentSearchApi.resolve('note', 'uuid')).toEqual({ title: 'Current title' });
  expect(invoke).toHaveBeenCalledExactlyOnceWith('resolve_search_target', { scope: 'note', uuid: 'uuid' });
});
it('propagates failed searches and stale destinations rather than claiming empty success', async () => {
  invoke.mockRejectedValue('SEARCH_DATABASE_FAILED');
  await expect(contentSearchApi.search('note', 'term')).rejects.toBe('SEARCH_DATABASE_FAILED');
  invoke.mockRejectedValue('SEARCH_UNAVAILABLE');
  await expect(contentSearchApi.resolve('attachment', 'uuid')).rejects.toBe('SEARCH_UNAVAILABLE');
});
