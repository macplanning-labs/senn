//! rooms.rs — 「どの行が、どの部屋に流れるか」「どの利用者が、どの部屋を購読するか」の規則
//!
//! 設計: docs/design/詳細設計書_リアルタイム同期_DeltaPush.md §5
//! 行の側（rooms_for_*）と利用者の側（rooms_for_access）は、`sync_repo::access_allows` と同じ判定になるよう
//! 対応させてある。ここを変えるときは access_allows も一緒に見直すこと。
use std::collections::BTreeSet;

use crate::domain::models::realtime::RoomId;
use crate::domain::models::sync_api::SyncAccessOut;

/// 全員が購読する部屋。プロジェクトは一覧 API と同じく権限では絞らないため、ここに流す。
pub fn global_room() -> RoomId {
    RoomId("g".to_string())
}

/// 利用者が購読する部屋の一覧。全員が入る `g` と、本人だけの `u:{user_id}`（通知）を含む
pub fn rooms_for_access(access: &SyncAccessOut, user_id: i32) -> Vec<RoomId> {
    let mut rooms = BTreeSet::new();
    rooms.insert(global_room());
    rooms.insert(RoomId::user(user_id));
    if access.all {
        rooms.insert(RoomId::all());
    } else {
        for t in &access.team_ids {
            rooms.insert(RoomId::team(*t));
        }
        for s in &access.scoped_projects {
            rooms.insert(RoomId::project(s.team_id, s.project_id));
        }
    }
    rooms.into_iter().collect()
}

/// チケット（チーム・プロジェクト）が流れる部屋。チームの無いチケットは管理者だけが見られる。
pub fn rooms_for_ticket(team_id: Option<i32>, project_id: Option<i32>) -> BTreeSet<RoomId> {
    let mut rooms = BTreeSet::new();
    rooms.insert(RoomId::all());
    if let Some(t) = team_id {
        rooms.insert(RoomId::team(t));
        if let Some(p) = project_id {
            rooms.insert(RoomId::project(t, p));
        }
    }
    rooms
}

/// プロジェクトの持ち物（サイクル・Wiki）が流れる部屋。
/// プロジェクトなし（全体の Wiki）は全員。プロジェクトあり: 管理者 / 所属チームのメンバー / そのプロジェクト限定のメンバー。
/// `access_allows` をチームごとに適用した結果と同じになる。
pub fn rooms_for_project(project_id: Option<i32>, team_ids: &[i32]) -> BTreeSet<RoomId> {
    let mut rooms = BTreeSet::new();
    let Some(p) = project_id else {
        rooms.insert(global_room());
        return rooms;
    };
    rooms.insert(RoomId::all());
    for &t in team_ids {
        rooms.insert(RoomId::team(t));
        rooms.insert(RoomId::project(t, p));
    }
    rooms
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::sync_api::ScopedProjectOut;

    fn access(all: bool, teams: &[i32], scoped: &[(i32, i32)]) -> SyncAccessOut {
        SyncAccessOut {
            all,
            team_ids: teams.to_vec(),
            scoped_projects: scoped
                .iter()
                .map(|&(team_id, project_id)| ScopedProjectOut {
                    team_id,
                    project_id,
                })
                .collect(),
        }
    }

    fn ids(v: impl IntoIterator<Item = RoomId>) -> Vec<String> {
        v.into_iter().map(|r| r.0).collect()
    }

    #[test]
    fn staff_subscribes_all_and_global() {
        assert_eq!(
            ids(rooms_for_access(&access(true, &[], &[]), 9)),
            vec!["all", "g", "u:9"]
        );
    }

    #[test]
    fn member_subscribes_team_and_scoped_rooms() {
        let r = ids(rooms_for_access(&access(false, &[3, 5], &[(7, 12)]), 9));
        assert_eq!(r, vec!["g", "p:7:12", "t:3", "t:5", "u:9"]);
    }

    #[test]
    fn ticket_rooms_follow_access_allows() {
        assert_eq!(
            ids(rooms_for_ticket(Some(3), Some(12))),
            vec!["all", "p:3:12", "t:3"]
        );
        assert_eq!(ids(rooms_for_ticket(Some(3), None)), vec!["all", "t:3"]);
        // チームの無いチケットは管理者だけ（access_allows と同じ）
        assert_eq!(ids(rooms_for_ticket(None, Some(12))), vec!["all"]);
    }

    /// 行が流れる部屋と、利用者が購読する部屋が交わる ⇔ access_allows が true
    #[test]
    fn matches_access_allows_exhaustively() {
        use crate::infrastructure::repositories::sync_repo::access_allows;
        let accesses = [
            access(true, &[], &[]),
            access(false, &[3], &[]),
            access(false, &[], &[(3, 12)]),
            access(false, &[4], &[(3, 13)]),
            access(false, &[], &[]),
        ];
        let places = [
            (None, None),
            (None, Some(12)),
            (Some(3), None),
            (Some(3), Some(12)),
            (Some(3), Some(13)),
            (Some(4), Some(12)),
        ];
        for a in &accesses {
            let subscribed: BTreeSet<RoomId> = rooms_for_access(a, 9).into_iter().collect();
            for &(t, p) in &places {
                let row_rooms = rooms_for_ticket(t, p);
                let via_rooms = row_rooms.iter().any(|r| subscribed.contains(r));
                assert_eq!(
                    via_rooms,
                    access_allows(a, t, p),
                    "access={a:?} team={t:?} project={p:?}"
                );
            }
        }
    }

    #[test]
    fn project_rooms_cover_admins_members_and_scoped_members() {
        assert_eq!(
            ids(rooms_for_project(Some(12), &[3, 5])),
            vec!["all", "p:3:12", "p:5:12", "t:3", "t:5"]
        );
        // 所属チームが無いプロジェクトは管理者だけ
        assert_eq!(ids(rooms_for_project(Some(12), &[])), vec!["all"]);
        // 全体の Wiki（プロジェクトなし）は全員
        assert_eq!(ids(rooms_for_project(None, &[])), vec!["g"]);
    }

    /// プロジェクトの持ち物は、そのプロジェクトのチケットを見られる人にだけ届く
    #[test]
    fn project_rooms_match_access_allows_for_every_team_of_the_project() {
        use crate::infrastructure::repositories::sync_repo::access_allows;
        let accesses = [
            access(true, &[], &[]),
            access(false, &[3], &[]),
            access(false, &[5], &[]),
            access(false, &[], &[(3, 12)]),
            access(false, &[], &[(3, 13)]),
            access(false, &[4], &[]),
        ];
        for a in &accesses {
            let subscribed: BTreeSet<RoomId> = rooms_for_access(a, 9).into_iter().collect();
            let via_rooms = rooms_for_project(Some(12), &[3, 5])
                .iter()
                .any(|r| subscribed.contains(r));
            let via_access = [3, 5].iter().any(|&t| access_allows(a, Some(t), Some(12)));
            assert_eq!(via_rooms, via_access, "access={a:?}");
        }
    }
}
