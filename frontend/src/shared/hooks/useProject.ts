/**
 * useProject.ts — プロジェクトコンテキストフック
 *
 * URLの :projectKey パラメータからプロジェクト情報を取得。
 * 最後に選択したプロジェクトを localStorage に保存し、
 * /dashboard 等のグローバルページからの遷移時に復元する。
 */

import { useParams, useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { apiClient } from '@/shared/api/client';
import { useCallback, useEffect } from 'react';

const LAST_PROJECT_KEY = 'wip-last-project-key';

export interface Project {
  id: number;
  name: string;
  prefix: string;
  description: string;
  ticketCount?: number;
  memberCount?: number;
  ownerId?: number | null;
  cycleAutoComplete?: boolean;
  cycleAutoCreateNext?: boolean;
  teams?: { id: number; name: string; slug: string; icon: string; color: string }[];
}

/** URLの :projectKey からプロジェクト情報を取得 */
export function useProject() {
  const { projectKey } = useParams<{ projectKey: string }>();

  // プロジェクト一覧を取得
  const { data: projects, isLoading: projectsLoading } = useQuery<{ results: Project[] }>({
    queryKey: ['projects'],
    queryFn: async () => {
      const res = await apiClient.get<{ results: Project[] }>('/projects/');
      return res.data;
    },
    staleTime: 1000 * 60 * 10, // 10分キャッシュ
  });

  const projectList = projects?.results ?? [];

  // URLのプロジェクトキーからプロジェクトを特定
  const currentProject = projectKey
    ? projectList.find((p) => p.prefix.toLowerCase() === projectKey.toLowerCase())
    : null;

  // 最後に選択したプロジェクトキーを保存
  useEffect(() => {
    if (projectKey) {
      localStorage.setItem(LAST_PROJECT_KEY, projectKey);
    }
  }, [projectKey]);

  return {
    /** URLの生のプロジェクトキー */
    projectKey: projectKey ?? null,
    /** 現在のプロジェクトオブジェクト（APIから取得） */
    currentProject,
    /** 全プロジェクト一覧 */
    projectList,
    /** 読み込み中 */
    isLoading: projectsLoading,
  };
}

/** 最後に使ったプロジェクトキーを取得（リダイレクト用） */
export function getLastProjectKey(): string | null {
  return localStorage.getItem(LAST_PROJECT_KEY);
}

/** プロジェクト切替フック */
export function useProjectSwitch() {
  const navigate = useNavigate();

  const switchProject = useCallback(
    (prefix: string) => {
      localStorage.setItem(LAST_PROJECT_KEY, prefix);
      navigate(`/project/${prefix}/tickets`);
    },
    [navigate],
  );

  return { switchProject };
}
