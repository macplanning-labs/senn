import { describe, expect, it, vi } from 'vitest';

vi.mock('../../api/client', () => {
  const apiClient = { get: vi.fn(), post: vi.fn(), defaults: { baseURL: '/api/v1' } };
  return { apiClient, default: apiClient };
});

import { backoffMs, buildWsUrl } from './realtimeClient';

describe('buildWsUrl', () => {
  it('相対の baseURL は現在のオリジンから ws/wss を作る', () => {
    expect(buildWsUrl('abc', '/api/v1', 'https://senn.example.com')).toBe('wss://senn.example.com/api/v1/realtime/ws?token=abc');
    expect(buildWsUrl('abc', '/api/v1', 'http://localhost:5173')).toBe('ws://localhost:5173/api/v1/realtime/ws?token=abc');
  });
  it('絶対の baseURL（デスクトップアプリなど）はそのホストを使う', () => {
    expect(buildWsUrl('a b', 'https://api.example.com/api/v1/', 'http://localhost')).toBe('wss://api.example.com/api/v1/realtime/ws?token=a+b');
  });
});

describe('backoffMs', () => {
  it('指数で増え、上限で頭打ちになり、揺らぎ（0.5〜1.5倍）が付く', () => {
    expect(backoffMs(1, 0.5)).toBe(500);
    expect(backoffMs(2, 0.5)).toBe(1000);
    expect(backoffMs(3, 0.5)).toBe(2000);
    expect(backoffMs(20, 0.5)).toBe(30_000);
    expect(backoffMs(3, 0)).toBe(1000);
    expect(backoffMs(3, 1)).toBe(3000);
  });
});
