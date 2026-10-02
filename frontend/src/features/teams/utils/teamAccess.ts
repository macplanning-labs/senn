/**
 * teamAccess.ts — チームの公開区分・参加状態による振り分け(アクセス制御の再設計 G-2)
 *
 * サーバーが返す `visibility`・`viewerIsMember` だけを見る(権限の判定はフロントでしない)。
 * 古いサーバーの応答(項目が無い)では、今までどおり全チームを参加済みとして扱う。
 */
import type { Team } from '@/shared/api/types';

type TeamLike = Pick<Team, 'visibility' | 'viewerIsMember' | 'archivedAt'>;

/** Private チームか */
export function isPrivateTeam(team: Pick<Team, 'visibility'>): boolean {
  return team.visibility === 'private';
}

/** 参加しているか(項目が無ければ参加済みとみなす) */
export function isJoined(team: Pick<Team, 'viewerIsMember'>): boolean {
  return team.viewerIsMember !== false;
}

/** サイドバーに出すチーム(参加済み) */
export function joinedTeams<T extends TeamLike>(teams: T[] | null | undefined): T[] {
  return (teams ?? []).filter(isJoined);
}

/** 「チームを探す」に出すチーム(Public で、未参加で、アーカイブされていない) */
export function discoverableTeams<T extends TeamLike>(teams: T[] | null | undefined): T[] {
  return (teams ?? []).filter(
    (t) => !isJoined(t) && !isPrivateTeam(t) && !t.archivedAt,
  );
}
