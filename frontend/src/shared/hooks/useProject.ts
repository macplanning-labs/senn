/**
 * useProject.ts — プロジェクトコンテキストフック
 *
 * URLの :projectKey パラメータからプロジェクト情報を取得。
 * 最後に選択したプロジェクトを localStorage に保存し、
 * /dashboard 等のグローバルページからの遷移時に復元する。
 */

import { useParams, useNavigate } from 'react-router-dom';
import { useCallback, useEffect } from 'react';
import { useProjects, useProjectByPrefix } from '@/shared/sync/repos/projectRepo';

const LAST_PROJECT_KEY = 'wip-last-project-key';

export interface Project {
  id: number;
  name: string;
  prefix: string;
  description: string;
  status?: string;
  priority?: string;
  targetEndDate?: string | null;
  ticketCount?: number;
  memberCount?: number;
  isMember: boolean;
  ownerId?: number | null;
  cycleAutoComplete?: boolean;
  cycleAutoCreateNext?: boolean;
  teams?: { id: number; name: string; slug: string; icon: string; color: string; archived?: boolean }[];
  // 階層・ロードマップ関連（後方互換: 任意）
  parentProjectId?: number | null;
  childCount?: number;
  roadmapIds?: number[];
}

/** URLの :projectKey からプロジェクト情報を取得 */
export function useProject() {
  const { projectKey } = useParams<{ projectKey: string }>();

  // プロジェクト一覧を取得（端末内 DB から）
  const { projects: projectList, isLoading } = useProjects();

  // URLのプロジェクトキーからプロジェクトを特定
  const currentProject = useProjectByPrefix(projectKey);

  // 最後に選択したプロジェクトキーを保存
  useEffect(() => {
    if (projectKey) {
      localStorage.setItem(LAST_PROJECT_KEY, projectKey);
    }
  }, [projectKey]);

  return {
    /** URLの生のプロジェクトキー */
    projectKey: projectKey ?? null,
    /** 現在のプロジェクトオブジェクト（端末内DBから取得） */
    currentProject: currentProject ?? null,
    /** 全プロジェクト一覧 */
    projectList,
    /** 読み込み中 */
    isLoading,
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
