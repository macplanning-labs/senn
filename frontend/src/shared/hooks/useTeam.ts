/**
 * useTeam.ts — Teamコンテキストフック
 *
 * URLの :teamSlug パラメータからTeam情報を取得。
 * useProject に相当する Team ベースの hook。
 */

import { useParams, useNavigate } from 'react-router-dom';
import { useCallback, useEffect, useMemo } from 'react';
import { useTeams } from '@/features/teams/hooks/useTeams';
import { activeTeams as filterActiveTeams, archivedTeams as filterArchivedTeams } from '@/features/teams/utils/archivedTeams';
import type { Team } from '@/shared/api/types';

export type { Team };

const LAST_TEAM_SLUG = 'wip-last-team-slug';

/** URLの :teamSlug からTeam情報を取得 */
export function useTeam() {
  const { teamSlug } = useParams<{ teamSlug: string }>();

  // Team一覧を取得（useTeamsとキャッシュを共有する。クエリキーが同じ['teams']でも
  // 片方が生のページネーション形式、片方が展開済み配列を返すと、先に解決した方の
  // 形でキャッシュが共有され、もう片方が配列以外を.map()して落ちる事故になるため、
  // 必ずuseTeamsを経由し、独自にqueryKey:['teams']のuseQueryを作らないこと）
  const { data: teams, isLoading: teamsLoading } = useTeams();

  const teamList = teams ?? [];

  // アーカイブされていないチームのみ
  const activeTeamList = useMemo(() => filterActiveTeams(teamList), [teamList]);

  // アーカイブ済みチームのみ
  const archivedTeamList = useMemo(() => filterArchivedTeams(teamList), [teamList]);

  // URLのteamSlugからTeamを特定（全チームから検索）
  const currentTeam = teamSlug
    ? teamList.find((t) => t.slug.toLowerCase() === teamSlug.toLowerCase())
    : null;

  // 最後に選択したTeamSlugを保存
  useEffect(() => {
    if (teamSlug) {
      localStorage.setItem(LAST_TEAM_SLUG, teamSlug);
    }
  }, [teamSlug]);

  return {
    /** URLの生のTeamSlug */
    teamSlug: teamSlug ?? null,
    /** 現在のTeamオブジェクト（APIから取得） */
    currentTeam,
    /** 全Team一覧 */
    teamList,
    /** アーカイブされていないチーム一覧 */
    activeTeams: activeTeamList,
    /** アーカイブ済みチーム一覧 */
    archivedTeams: archivedTeamList,
    /** 読み込み中 */
    isLoading: teamsLoading,
  };
}

/** 最後に使ったTeamSlugを取得（リダイレクト用） */
export function getLastTeamSlug(): string | null {
  return localStorage.getItem(LAST_TEAM_SLUG);
}

/** サイドバーなどから Team 画面へ切り替える */
export function useTeamSwitch() {
  const navigate = useNavigate();
  const switchTeam = useCallback((slug: string) => {
    localStorage.setItem(LAST_TEAM_SLUG, slug);
    navigate(`/team/${slug}/tickets`);
  }, [navigate]);
  return { switchTeam };
}
