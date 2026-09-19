import { describe, expect, it } from 'vitest';
import { reviewDateDisplay, reviewDateValue } from './waitingReviewDate';
import { validReviewDate } from '$lib/stores/taskWorkflowStore';

describe('waiting review date presentation', () => {
  it('displays a fixed slash format but stores canonical ISO dates', () => {
    for (const date of ['', '0001-01-01', '2024-02-29', '9999-12-31']) {
      expect(reviewDateValue(reviewDateDisplay(date))).toBe(date);
      expect(validReviewDate(reviewDateValue(reviewDateDisplay(date)))).toBe(true);
    }
    expect(reviewDateDisplay('2026-09-19')).toBe('2026/09/19');
    expect(reviewDateValue('2026/09/19')).toBe('2026-09-19');
  });
  it('does not guess, trim or silently clear invalid input', () => {
    for (const input of ['2026/02/29', '2026/9/1', '2026/09', ' 2026/09/19', '0000/01/01']) {
      expect(validReviewDate(reviewDateValue(input))).toBe(false);
      expect(reviewDateValue(input)).not.toBe('');
    }
  });
});
