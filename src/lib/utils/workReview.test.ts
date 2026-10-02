import { afterEach, describe, expect, it, vi } from 'vitest';
import fixture from '../../../docs/fixtures/work-review-v1.json';
import type { ReviewQuery } from '$lib/types/workReview';
import { translate, type ResolvedLocale } from '$lib/i18n';
import { compareReviewRows, formatReviewSummary, normalizeReviewQuery, reviewDateRange, reviewPeriodDates, REVIEW_TEXT_LIMIT } from './workReview';
afterEach(() => vi.unstubAllEnvs());

describe('local review calendar and shared summary contract', () => {
  it('uses Monday weeks across year boundaries and an inclusive custom end', () => {
    expect(reviewPeriodDates('week', new Date(2027, 0, 3, 20))).toEqual({ start: '2026-12-28', end: '2027-01-03' });
    expect(reviewPeriodDates('previousWeek', new Date(2027, 0, 3))).toEqual({ start: '2026-12-21', end: '2026-12-27' });
    expect(reviewPeriodDates('today', new Date(2026, 9, 2, 23))).toEqual({ start: '2026-10-02', end: '2026-10-02' });
    const range = reviewDateRange({ start: '2024-02-29', end: '2024-02-29' })!;
    expect(new Date(range.start_at).getDate()).toBe(29);
    expect(new Date(range.end_at).getMonth()).toBe(2);
    expect(new Date(range.end_at).getDate()).toBe(1);
  });
  it('rejects reversed, impossible, missing and noncanonical dates', () => {
    for (const dates of [{ start: '2026-10-03', end: '2026-10-02' }, { start: '2026-02-30', end: '2026-03-01' },
      { start: '', end: '2026-10-02' }, { start: '2026-1-02', end: '2026-10-02' }]) expect(reviewDateRange(dates)).toBeNull();
  });
  it('matches shared 23/25-hour DST boundaries rather than adding 86400000', () => {
    for (const vector of fixture.date_cases) {
      vi.stubEnv('TZ', vector.time_zone);
      const range = reviewDateRange(vector.dates)!;
      expect(range).toEqual({ start_at: vector.start_at, end_at: vector.end_at });
      expect((range.end_at - range.start_at) / 3600000).toBe(vector.hours);
    }
  });
  it('rejects negative UTC starts and accepts the exact nonnegative epoch boundary', () => {
    vi.stubEnv('TZ', 'UTC');
    expect(reviewDateRange({ start: '1969-12-31', end: '1970-01-01' })).toBeNull();
    expect(reviewDateRange({ start: '1970-01-01', end: '1970-01-01' })).toEqual({ start_at: 0, end_at: 86400000 });
    vi.stubEnv('TZ', 'Asia/Shanghai');
    expect(reviewDateRange({ start: '1970-01-01', end: '1970-01-01' })).toBeNull();
    expect(reviewDateRange({ start: '1970-01-02', end: '1970-01-02' })!.start_at).toBeGreaterThan(0);
  });
  it('normalizes only ASCII case and trim, retaining literal wildcard characters', () => {
    for (const vector of fixture.keyword_cases) expect(normalizeReviewQuery({ ...fixture.query, keyword: vector.input } as ReviewQuery).keyword).toBe(vector.normalized);
    expect(normalizeReviewQuery({ ...fixture.query, group_uuid: 'ignored', keyword: ' Ä中文AZ ' } as ReviewQuery)).toMatchObject({ keyword: 'Ä中文az', group_uuid: null });
  });
  it('sorts equal times by record UUID, without locale-sensitive comparison', () => {
    const [a, b] = fixture.snapshot.rows;
    expect(compareReviewRows({ ...a, created_at: 1 }, { ...b, created_at: 1 })).toBe(-1);
  });
  it('matches the shared golden summaries, dates then task latest record, preserving full text', () => {
    vi.stubEnv('TZ', fixture.time_zone);
    for (const locale of ['zh-CN', 'en-US'] as ResolvedLocale[]) {
      const text = formatReviewSummary(fixture.snapshot, { dates: fixture.dates, locale, groupName: translate(locale, 'workReview.allGroups') });
      expect(text).toBe(fixture.expected_summary[locale]);
      expect(formatReviewSummary({ ...fixture.snapshot, rows: [...fixture.snapshot.rows].reverse() },
        { dates: fixture.dates, locale, groupName: translate(locale, 'workReview.allGroups') })).toBe(text);
    }
  });
  it('allows exactly 100000 UTF-16 units including headers, never truncates oversized text', () => {
    const options = { dates: fixture.dates, locale: 'en-US' as const, groupName: 'All groups' };
    const row = { ...fixture.snapshot.rows[0], body: '' };
    const snapshot = { ...fixture.snapshot, matching_entry_count: 1, matching_task_count: 1, rows: [row] };
    const headerSize = formatReviewSummary(snapshot, options).length;
    row.body = 'x'.repeat(REVIEW_TEXT_LIMIT - headerSize);
    expect(formatReviewSummary(snapshot, options).length).toBe(REVIEW_TEXT_LIMIT);
    row.body += 'x'; expect(() => formatReviewSummary(snapshot, options)).toThrow('REVIEW_LIMIT');
    row.body = '😀'.repeat(50000); expect(() => formatReviewSummary(snapshot, options)).toThrow('REVIEW_LIMIT');
  });
  it('normalizes CRLF without dropping lines or applying HTML interpretation', () => {
    const row = { ...fixture.snapshot.rows[0], body: '<b>plain</b>\r\nsecond\rthird' };
    expect(formatReviewSummary({ ...fixture.snapshot, rows: [row] }, { dates: fixture.dates, locale: 'en-US', groupName: 'All' })).toContain('<b>plain</b>\n  second\n  third');
  });
});
