// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import type { ReactNode } from 'react';

vi.mock('@/shared/api/client', () => ({
  apiClient: { get: vi.fn() },
}));

import { apiClient } from '@/shared/api/client';
import { useStatusOptionsForProjects } from './useStatusOptionsForProjects';

const mockGet = vi.mocked(apiClient.get);

const std = [
  { slug: 'open', name: 'Todo', color: '#111' },
  { slug: 'closed', name: 'Closed', color: '#222' },
];

function wrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe('useStatusOptionsForProjects', () => {
  beforeEach(() => {
    mockGet.mockReset();
    mockGet.mockImplementation(async (_url, config) => {
      const project = (config?.params as { project?: number } | undefined)?.project;
      const results = project === 2 ? [std[0], { slug: 'review', name: 'Review', color: '#abc' }, std[1]] : std;
      return { data: { results } };
    });
  });

  it('プロジェクトに追加したステータス(review)が和集合・表示名・色に現れる', async () => {
    const { result } = renderHook(() => useStatusOptionsForProjects([1, 2, 2]), { wrapper: wrapper() });
    await waitFor(() => expect(result.current.union.some((o) => o.value === 'review')).toBe(true));
    expect(result.current.union.map((o) => o.value)).toEqual(['open', 'closed', 'review']);
    expect(result.current.labelOf('review', 2)).toBe('Review');
    expect(result.current.colorOf('review', 2)).toBe('#abc');
    expect(result.current.optionsFor(1).some((o) => o.value === 'review')).toBe(false);
    expect(mockGet).toHaveBeenCalledTimes(2);
  });

  it('未取得・不明なプロジェクトは標準の名前', () => {
    const { result } = renderHook(() => useStatusOptionsForProjects([]), { wrapper: wrapper() });
    expect(result.current.labelOf('open', null)).toBe('Todo');
    expect(result.current.labelOf('review', 99)).toBe('review');
    expect(result.current.colorOf('review', 99)).toBe('inherit');
  });
});
