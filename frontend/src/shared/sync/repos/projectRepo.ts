/**
 * projectRepo.ts — 画面がプロジェクトの「行」を読むためのフック（端末内 DB だけを見る）
 *
 * 詳細設計 §6。並び順はサーバーの一覧（名前の昇順）と同じ。
 */
import { useMemo } from 'react';
import { db, type LocalProject } from '../db';
import { useSyncStatus } from '../syncStatusStore';
import { useLiveRows } from './useLiveRows';

const EMPTY: LocalProject[] = [];

function byName(a: LocalProject, b: LocalProject): number {
  if (a.name === b.name) return a.id - b.id;
  return a.name < b.name ? -1 : 1;
}

/** 全プロジェクト（削除待ちを除く、名前順） */
export function useProjects(): { projects: LocalProject[]; isLoading: boolean } {
  const initialSyncDone = useSyncStatus((s) => s.initialSyncDone);
  const { data, loaded } = useLiveRows(
    async () => (await db.projects.toArray()).filter((p) => !p._deleted).sort(byName),
    [],
    EMPTY,
  );
  return { projects: data, isLoading: !loaded || (!initialSyncDone && data.length === 0) };
}

export function useProjectById(id: number | null | undefined): LocalProject | undefined {
  const { projects } = useProjects();
  return useMemo(() => (id == null ? undefined : projects.find((p) => p.id === id)), [projects, id]);
}

/** プレフィックスは大文字小文字を区別しない（URL の :projectKey と同じ扱い） */
export function useProjectByPrefix(prefix: string | null | undefined): LocalProject | undefined {
  const { projects } = useProjects();
  return useMemo(
    () => (prefix ? projects.find((p) => p.prefix.toLowerCase() === prefix.toLowerCase()) : undefined),
    [projects, prefix],
  );
}
