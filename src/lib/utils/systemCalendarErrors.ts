const kinds = {
  CALENDAR_DENIED: 'denied',
  CALENDAR_CREDENTIALS: 'denied',
  CALENDAR_INVALID_DOCUMENT: 'invalid',
  CALENDAR_INVALID_RESPONSE: 'invalid',
  CALENDAR_DOCUMENT_TOO_LARGE: 'invalid',
  CALENDAR_UNSUPPORTED_VERSION: 'invalid',
  CALENDAR_ETAG_REQUIRED: 'invalid',
  CALENDAR_REVISION_REGRESSION: 'invalid',
  CALENDAR_SOURCE_MISSING: 'missing',
  CALENDAR_TARGET_CHANGED: 'target',
  CALENDAR_CONFIG_CHANGED: 'target',
  CALENDAR_NETWORK: 'network',
  CALENDAR_CACHE: 'cache',
} as const;

export function calendarErrorCode(reason: unknown): string {
  const message = reason instanceof Error ? reason.message : String(reason);
  const code = message.match(/\bCALENDAR_[A-Z_]+\b/)?.[0];
  return code && Object.prototype.hasOwnProperty.call(kinds, code) ? code : 'CALENDAR_REFRESH_FAILED';
}

export function calendarErrorKind(error: string) {
  const code = calendarErrorCode(error);
  return kinds[code as keyof typeof kinds] ?? 'generic';
}
