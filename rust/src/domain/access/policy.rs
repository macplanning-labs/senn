//! policy — 「この閲覧者は、このリソースに、この操作をしてよいか」
//!
//! 規則の表: docs/design/詳細設計書_アクセス制御_再設計.md §5
//! - 見えない物は `NotFound`(404。存在を推測させない)
//! - 見えるが操作できない物は `Forbidden`(403)

use super::resource::{ResourceRef, SettingsPolicy, TeamFacts, Visibility, WikiScope};
use super::viewer::{Role, Viewer};

/// 操作の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Read,
    Write,
    Create,
    Delete,
    /// 一括削除(キー経由は禁止)
    BulkDelete,
    /// チームの設定(ワークフロー・ラベル・ルール・連携・アーカイブなど)
    ManageSettings,
    /// メンバー・Guest・Owner の管理
    ManageMembers,
    /// Owner の指名・解除と、設定の方針の変更(方針にかかわらず Owner とシステム管理者だけ。詳細設計書 §5.4)
    ManageOwners,
    /// 公開区分の変更
    ChangeVisibility,
    /// 招待
    Invite,
    /// Public チームへの参加
    Join,
}

/// 判定の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// 見えない(404)
    NotFound,
    /// 見えるが操作できない(403)
    Forbidden,
}

impl Decision {
    pub fn is_allowed(self) -> bool {
        self == Decision::Allow
    }

    fn from_bool(ok: bool) -> Self {
        if ok {
            Decision::Allow
        } else {
            Decision::Forbidden
        }
    }
}

/// キー経由(個人キー・外部連携)では禁止する操作(設計書 §10)。チケット単体の削除は許す(§15-12)
fn forbidden_for_keys(action: Action) -> bool {
    matches!(
        action,
        Action::BulkDelete
            | Action::ManageSettings
            | Action::ManageMembers
            | Action::ManageOwners
            | Action::ChangeVisibility
            | Action::Invite
            | Action::Join
    )
}

/// 判定の入口。見え方の規則は、すべてここを通す
pub fn can(viewer: &Viewer, action: Action, res: &ResourceRef) -> Decision {
    if !can_read(viewer, res) {
        return Decision::NotFound;
    }
    if viewer.principal.is_key() && forbidden_for_keys(action) {
        return Decision::Forbidden;
    }
    if action == Action::Read {
        return Decision::Allow;
    }
    Decision::from_bool(can_act(viewer, action, res))
}

/// チームを見られるか(すべての規則の土台。設計書 §5.1)
pub fn sees_team(viewer: &Viewer, team: &TeamFacts) -> bool {
    if viewer.team_membership(team.team_id).is_some() {
        return true;
    }
    team.visibility == Visibility::Public && viewer.is_full_member()
}

/// Owner の操作ができるか(詳細設計書 §5.4・設計書 §4.3。DEMO-000169)
///
/// Owner の指名・解除、設定の方針・公開区分の変更、チームの削除・アーカイブ、招待、Guest の追加。
/// `settings_policy` は見ない。Owner が 0 人のチームでは、システム管理者だけ(Join しただけの人に Owner を決めさせない)。
pub fn manages_owners(viewer: &Viewer, team: &TeamFacts) -> bool {
    if viewer.principal.is_key() {
        return false;
    }
    match viewer.role {
        Role::Guest | Role::Integration => false,
        Role::SystemAdmin => true,
        Role::FullMember => viewer
            .team_membership(team.team_id)
            .is_some_and(|m| m.is_owner),
    }
}

/// チームの設定を管理できるか(詳細設計書 §5.3)。Owner の操作には使わない(`manages_owners`)
pub fn manages_team(viewer: &Viewer, team: &TeamFacts) -> bool {
    if viewer.principal.is_key() {
        return false;
    }
    match viewer.role {
        Role::Guest | Role::Integration => return false,
        Role::SystemAdmin if team.visibility == Visibility::Public => return true,
        _ => {}
    }
    let Some(m) = viewer.team_membership(team.team_id) else {
        return false;
    };
    match team.settings_policy {
        SettingsPolicy::Members => true,
        // Owner が 0 人になったら、全員に戻す(チームを詰まらせない。設計書 §4.3)
        SettingsPolicy::Owners => m.is_owner || team.owner_count == 0,
    }
}

fn can_read(viewer: &Viewer, res: &ResourceRef) -> bool {
    match res {
        ResourceRef::Team(t) => sees_team(viewer, t),
        ResourceRef::Ticket {
            team,
            project_id,
            author_id,
            assignee_ids,
        } => match team {
            Some(t) => {
                sees_team(viewer, t)
                    || project_id.is_some_and(|p| viewer.has_project_membership(t.team_id, p))
            }
            // チームの無いチケット: 作成者・担当者だけ(システム管理者も例外にしない)
            None => viewer
                .user_id()
                .is_some_and(|u| *author_id == Some(u) || assignee_ids.contains(&u)),
        },
        ResourceRef::Project { project_id, teams } => {
            teams.iter().any(|t| sees_team(viewer, t))
                || viewer.has_any_project_membership(*project_id)
        }
        // サイクル・Wiki は、プロジェクト単位の所属では見えない(チームの規則だけ)
        ResourceRef::Cycle { team } => sees_team(viewer, team),
        ResourceRef::Wiki { scope, .. } => match scope {
            WikiScope::Team(t) => sees_team(viewer, t),
            WikiScope::Project { teams, .. } => teams.iter().any(|t| sees_team(viewer, t)),
            WikiScope::Global => viewer.is_full_member(),
        },
        ResourceRef::GlobalMaster => true,
        // ロードマップ: Full Member だけ(含まれるプロジェクトは、プロジェクトの規則で絞る)
        ResourceRef::Roadmap => viewer.is_full_member(),
        ResourceRef::User {
            user_id,
            is_guest,
            team_ids,
            project_grants,
        } => can_read_user(viewer, *user_id, *is_guest, team_ids, project_grants),
    }
}

fn can_read_user(
    viewer: &Viewer,
    user_id: i32,
    is_guest: bool,
    team_ids: &[i32],
    project_grants: &[(i32, i32)],
) -> bool {
    if viewer.user_id() == Some(user_id) {
        return true;
    }
    let shares_team = team_ids
        .iter()
        .any(|t| viewer.team_membership(*t).is_some());
    let shares_project = project_grants
        .iter()
        .any(|(t, p)| viewer.has_project_membership(*t, *p));
    match viewer.role {
        // Full Member: Full Member 全員と、自分が見られるチームの Guest(Private だけにいる Guest は含めない)
        Role::FullMember | Role::SystemAdmin => {
            if !is_guest {
                return true;
            }
            let visible = viewer.visible_team_ids();
            team_ids.iter().any(|t| visible.contains(t))
                || project_grants.iter().any(|(t, _)| visible.contains(t))
        }
        // Guest: 同じチーム、または同じプロジェクトの所属を持つ人だけ
        Role::Guest => shares_team || shares_project,
        Role::Integration => shares_team,
    }
}

fn can_act(viewer: &Viewer, action: Action, res: &ResourceRef) -> bool {
    let me = viewer.user_id();
    match res {
        ResourceRef::Team(t) => match action {
            Action::ManageSettings | Action::ManageMembers => manages_team(viewer, t),
            Action::ManageOwners | Action::ChangeVisibility | Action::Invite => {
                manages_owners(viewer, t)
            }
            Action::Join => {
                viewer.is_full_member()
                    && t.visibility == Visibility::Public
                    && viewer.team_membership(t.team_id).is_none()
            }
            // チームそのものの作成・削除は、それぞれの API で扱う(削除・アーカイブは Owner の操作)
            Action::Delete => manages_owners(viewer, t),
            _ => false,
        },
        ResourceRef::Ticket {
            team, author_id, ..
        } => match action {
            Action::Write | Action::Create => true,
            Action::Delete | Action::BulkDelete => {
                let is_author = me.is_some() && *author_id == me;
                is_author || team.as_ref().is_some_and(|t| manages_team(viewer, t))
            }
            _ => false,
        },
        ResourceRef::Project { teams, .. } => match action {
            // 作成: 参加させるすべてのチームで、チームが見えること(Guest は不可)
            Action::Create => {
                viewer.role != Role::Guest
                    && !teams.is_empty()
                    && teams.iter().all(|t| sees_team(viewer, t))
            }
            Action::Write => {
                viewer.role != Role::Guest && teams.iter().any(|t| sees_team(viewer, t))
            }
            Action::Delete | Action::ManageSettings => {
                teams.iter().any(|t| manages_team(viewer, t))
            }
            _ => false,
        },
        ResourceRef::Cycle { team } => match action {
            Action::Write | Action::Create | Action::Delete => sees_team(viewer, team),
            _ => false,
        },
        ResourceRef::Wiki { scope, author_id } => match action {
            Action::Write | Action::Create => true,
            Action::Delete => {
                let is_author = me.is_some() && *author_id == me;
                is_author
                    || match scope {
                        WikiScope::Team(t) => manages_team(viewer, t),
                        WikiScope::Project { teams, .. } => {
                            teams.iter().any(|t| manages_team(viewer, t))
                        }
                        WikiScope::Global => viewer.is_system_admin(),
                    }
            }
            _ => false,
        },
        ResourceRef::GlobalMaster => {
            matches!(action, Action::Write | Action::Create | Action::Delete)
                && viewer.is_system_admin()
        }
        // ロードマップの編集・削除: Full Member(見えること = Full Member)。作成者(owner)に特別な権限は無い
        ResourceRef::Roadmap => matches!(action, Action::Write | Action::Create | Action::Delete),
        ResourceRef::User { user_id, .. } => match action {
            Action::Write => me == Some(*user_id),
            Action::ManageSettings | Action::ManageMembers => viewer.is_system_admin(),
            _ => false,
        },
    }
}
