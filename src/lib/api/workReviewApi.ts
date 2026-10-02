import { invoke } from '@tauri-apps/api/core';
import type { ReviewCursor, ReviewPage, ReviewQuery, ReviewSnapshot } from '$lib/types/workReview';

export const workReviewApi = {
  list: (query: ReviewQuery, cursor: ReviewCursor | null = null) =>
    invoke<ReviewPage>('list_work_review', { query, cursor }),
  snapshot: (query: ReviewQuery) => invoke<ReviewSnapshot>('snapshot_work_review', { query }),
  validate: (query: ReviewQuery, snapshotToken: string) =>
    invoke<boolean>('validate_work_review', { query, snapshotToken }),
};
