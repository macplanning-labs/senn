import { describe, it, expect } from 'vitest';
import { AxiosError, AxiosHeaders } from 'axios';
import i18n from '@/i18n';
import { lockMinutes, rateLimitMessage } from './rateLimitMessage';

function httpError(status: number, data: unknown, headers: Record<string, string> = {}) {
  const config = { headers: new AxiosHeaders() };
  return new AxiosError('x', 'ERR_BAD_REQUEST', config, undefined, {
    status,
    statusText: '',
    headers,
    config,
    data,
  });
}

const t = (lng: string) => i18n.getFixedT(lng) as unknown as (k: string, o?: Record<string, unknown>) => string;

describe('rateLimitMessage', () => {
  it('試行ロック(account_locked)は、待つ分数つきの文言(日本語・英語)', () => {
    const err = httpError(429, { error: 'account_locked', retry_after_seconds: 823 });
    expect(rateLimitMessage(err, t('ja'))).toBe(
      'ログインの試行が多すぎます。14分ほど待ってから、もう一度お試しください。',
    );
    expect(rateLimitMessage(err, t('en'))).toBe(
      'Too many sign-in attempts. Please wait about 14 minute(s) and try again.',
    );
  });

  it('本文に秒数が無ければ Retry-After、それも無ければ 15 分', () => {
    const fromHeader = httpError(429, { error: 'account_locked' }, { 'retry-after': '60' });
    expect(rateLimitMessage(fromHeader, t('en'))).toContain('about 1 minute');
    const none = httpError(429, { error: 'account_locked' });
    expect(rateLimitMessage(none, t('ja'))).toContain('15分');
  });

  it('IP の率制限(detail だけの 429)は、一般的な「回数が多すぎる」文言', () => {
    const err = httpError(429, { detail: 'リクエストが多すぎます。3秒後に再試行してください。' });
    expect(rateLimitMessage(err, t('ja'))).toBe(i18n.getFixedT('ja')('auth.tooManyRequests'));
    expect(rateLimitMessage(err, t('en'))).toBe(i18n.getFixedT('en')('auth.tooManyRequests'));
  });

  it('429 以外は null(呼び出し側の既定の文言を使う)', () => {
    expect(rateLimitMessage(httpError(401, { detail: 'x' }), t('ja'))).toBeNull();
    expect(rateLimitMessage(new Error('network'), t('ja'))).toBeNull();
  });

  it('分数は切り上げ・最低 1 分', () => {
    expect(lockMinutes(1)).toBe(1);
    expect(lockMinutes(61)).toBe(2);
    expect(lockMinutes('900')).toBe(15);
    expect(lockMinutes(0)).toBeNull();
    expect(lockMinutes('abc')).toBeNull();
  });
});
