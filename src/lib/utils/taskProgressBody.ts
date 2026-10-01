export type ProgressBodyError = 'invalid' | 'limit';
export function normalizeProgressBody(input: string): { body: string; error: ProgressBodyError | null } {
  // ECMAScript trim and UTF-16 length are the frozen cross-client contract.
  const body = input.trim();
  if (body.length > 1000) return { body, error: 'limit' };
  if (!body || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/u.test(body)) {
    return { body, error: 'invalid' };
  }
  for (let i = 0; i < body.length; i++) {
    const unit = body.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = body.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) return { body, error: 'invalid' };
    } else if (unit >= 0xdc00 && unit <= 0xdfff) return { body, error: 'invalid' };
  }
  return { body, error: null };
}
