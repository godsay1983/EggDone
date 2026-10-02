import { expect, it, vi } from 'vitest';
import fixture from '../../../docs/fixtures/work-review-v1.json';
import type { ReviewQuery } from '$lib/types/workReview';
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));
import { workReviewApi } from './workReviewApi';

it('freezes query/cursor snake_case and named camelCase token IPC', async () => {
  invoke.mockReset();
  const query = fixture.query as ReviewQuery;
  const cursor = { created_at: 1, record_uuid: 'record', query_key: 'query', snapshot_token: 'snapshot' };
  await workReviewApi.list(query); await workReviewApi.list(query, cursor);
  await workReviewApi.snapshot(query); await workReviewApi.validate(query, 'snapshot');
  expect(invoke.mock.calls).toEqual([
    ['list_work_review', { query, cursor: null }], ['list_work_review', { query, cursor }],
    ['snapshot_work_review', { query }], ['validate_work_review', { query, snapshotToken: 'snapshot' }],
  ]);
});
it('propagates changed and database errors instead of empty results', async () => {
  invoke.mockRejectedValueOnce('REVIEW_CHANGED');
  await expect(workReviewApi.list(fixture.query as ReviewQuery)).rejects.toBe('REVIEW_CHANGED');
});
