export type CaptureTarget = 'todo' | 'note';

export interface CaptureInput {
  target: string;
  title: string;
  body: string;
  source_url: string;
  source_app: string;
}

export interface CaptureDraft {
  target: CaptureTarget;
  title: string;
  body: string;
  source_url: string;
  source_app: string;
  truncated: boolean;
}

export const CAPTURE_TITLE_LIMIT: number = 100;
export const CAPTURE_BODY_LIMIT: number = 20000;

function cleanText(value: string): string {
  return value.replace(/\r\n?/g, '\n')
    .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F-\u009F\u202A-\u202E\u2066-\u2069]/g, '').trim();
}

// UTF-16 limits match ArkUI inputs; never split a surrogate pair.
function clip(value: string, limit: number): string {
  let end = Math.min(value.length, limit);
  if (end > 0 && /[\uD800-\uDBFF]/.test(value.charAt(end - 1))) end--;
  return value.slice(0, end);
}

export function normalizeCapture(input: CaptureInput): CaptureDraft {
  if (typeof input.title !== 'string' || typeof input.body !== 'string' ||
    typeof input.source_url !== 'string' || typeof input.source_app !== 'string') throw new Error('CAPTURE_INVALID');
  if (input.target !== 'todo' && input.target !== 'note') throw new Error('CAPTURE_INVALID');
  if (input.title.length + input.body.length + input.source_url.length + input.source_app.length > 100000) {
    throw new Error('CAPTURE_TOO_LARGE');
  }
  const sourceUrl = input.source_url.trim();
  // Keep a URL literal. Never read local paths, decode commands, or fetch a page.
  if (/[\\\u0000-\u0020\u007F-\u009F\u202A-\u202E\u2066-\u2069]/.test(sourceUrl) ||
    sourceUrl.length > 2048 || (sourceUrl.length > 0 &&
    !/^https?:\/\/[^\s/?#:@]+(?::[0-9]{1,5})?(?:[/?#][^\s]*)?$/i.test(sourceUrl))) {
    throw new Error('CAPTURE_INVALID');
  }
  const cleanBody = cleanText(input.body);
  const cleanTitle = cleanText(input.title).replace(/\s+/g, ' ');
  const title = clip(cleanTitle, CAPTURE_TITLE_LIMIT);
  const body = clip(cleanBody, CAPTURE_BODY_LIMIT);
  const sourceApp = cleanText(input.source_app).replace(/\s+/g, ' ');
  return {
    target: input.target, title, body, source_url: sourceUrl,
    source_app: clip(sourceApp, 100),
    truncated: title.length !== cleanTitle.length || body.length !== cleanBody.length || sourceApp.length > 100
  };
}

export function captureContent(draft: CaptureDraft): string {
  const body = draft.body.trim();
  return draft.source_url.length === 0 || body === draft.source_url || body.endsWith('\n' + draft.source_url) ?
    body : body + (body.length > 0 ? '\n\n' : '') + draft.source_url;
}

export function captureTitle(draft: CaptureDraft): string {
  return draft.title.trim() || clip(draft.body.trim().split('\n')[0] || draft.source_url, CAPTURE_TITLE_LIMIT);
}
