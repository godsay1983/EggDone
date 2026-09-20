import { expect, it } from 'vitest';
import { calendarErrorCode, calendarErrorKind } from './systemCalendarErrors';

it('maps native calendar errors to localized categories without showing transport text', () => {
  expect(calendarErrorKind('CALENDAR_DENIED')).toBe('denied');
  expect(calendarErrorKind('CALENDAR_CREDENTIALS')).toBe('denied');
  for (const code of ['INVALID_DOCUMENT', 'INVALID_RESPONSE', 'UNSUPPORTED_VERSION', 'DOCUMENT_TOO_LARGE',
    'REVISION_REGRESSION', 'ETAG_REQUIRED']) expect(calendarErrorKind('CALENDAR_' + code)).toBe('invalid');
  expect(calendarErrorKind('CALENDAR_SOURCE_MISSING')).toBe('missing');
  expect(calendarErrorKind('CALENDAR_TARGET_CHANGED')).toBe('target');
  expect(calendarErrorKind('CALENDAR_NETWORK')).toBe('network');
  expect(calendarErrorKind('CALENDAR_CACHE')).toBe('cache');
  expect(calendarErrorCode(Error('CALENDAR_DENIED https://private.test'))).toBe('CALENDAR_DENIED');
  expect(calendarErrorCode('CALENDAR_UNKNOWN token=secret')).toBe('CALENDAR_REFRESH_FAILED');
  expect(calendarErrorKind('Unknown secret transport error')).toBe('generic');
});
