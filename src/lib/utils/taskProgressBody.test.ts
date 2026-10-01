import { describe, expect, it } from 'vitest';
import { normalizeProgressBody } from './taskProgressBody';
describe('progress body contract', () => {
  it('uses ECMAScript trim, preserving internal CRLF and combining characters', () => {
    expect(normalizeProgressBody('\uFEFF\u3000 A\r\n\te\u0301 \u00a0')).toEqual({ body: 'A\r\n\te\u0301', error: null });
  });
  it('counts normalized UTF-16 units, not scalars or graphemes', () => {
    expect(normalizeProgressBody('😀'.repeat(500)).error).toBeNull();
    expect(normalizeProgressBody('😀'.repeat(500) + 'x').error).toBe('limit');
    expect(normalizeProgressBody(' ' + 'x'.repeat(1000) + ' ').error).toBeNull();
  });
  it.each(['', ' \t\r\n\uFEFF', 'a\u0000b', 'a\u007fb', 'a\u0085b', 'a\u202ab', 'a\u2069b', '\ud800', '\udc00', '\ud800a'])('rejects %j', text => {
    expect(normalizeProgressBody(text).error).toBe('invalid');
  });
});
