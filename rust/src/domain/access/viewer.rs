//! 閲覧者(Viewer): 誰が、どの経路で、どの役割と所属を持って操作しているか

use chrono::NaiveDate;

/// どの経路で操作しているか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Principal {
    /// ブラウザ・デスクトップアプリ(ログインした人)
    Human { user_id: i32 },
    /// 個人のキー(AI キー・MCP を含む)。キーの持ち主として動く
    PersonalKey { user_id: i32, key_id: i64 },
    /// 外部連携。人の権限を借りず、許可されたチームだけで動く
    Integration { integration_id: i64 },
}

impl Principal {
    /// 人のユーザー ID(外部連携には無い)
    pub fn user_id(&self) -> Option<i32> {
        match *self {
            Principal::Human { user_id } | Principal::PersonalKey { user_id, .. } => Some(user_id),
            Principal::Integration { .. } => None,
        }
    }

    /// キー経由(個人キー・外部連携)か。破壊的な操作の禁止・回数制限の対象
    pub fn is_key(&self) -> bool {
        !matches!(self, Principal::Human { .. })
    }
}

/// 役割(ユーザーごとに 1 つ。外部連携は Integration)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// システム設定・ユーザー管理・孤立チームの救済。Private チームの中身は見えない
    SystemAdmin,
    /// 社内メンバー。Public チームのすべてを閲覧・書き込みできる
    FullMember,
    /// 招待されたチーム(またはプロジェクト)だけ
    Guest,
    /// 外部連携
    Integration,
}

/// 所属の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipKind {
    /// チーム全体の所属(`scoped_project_id IS NULL`)
    Team,
    /// プロジェクト単位の所属(SENN の拡張。そのプロジェクトのチケット・コメント・添付だけ)
    Project { project_id: i32 },
}

/// 有効な所属(期限切れの物は、読み込みの時点で除いてある)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Membership {
    pub team_id: i32,
    pub kind: MembershipKind,
    /// Owner か(DB の role = 'admin' をこう読み替える)
    pub is_owner: bool,
    /// 期限(end_date + プロジェクトの猶予日数)。無期限は None。リアルタイムの再計算の予約に使う
    pub expires_on: Option<NaiveDate>,
}

/// いま操作している主体
#[derive(Debug, Clone)]
pub struct Viewer {
    pub principal: Principal,
    pub role: Role,
    pub memberships: Vec<Membership>,
    /// Public チームの ID(Full Member の閲覧範囲)
    pub public_team_ids: Vec<i32>,
}

impl Viewer {
    pub fn user_id(&self) -> Option<i32> {
        self.principal.user_id()
    }

    /// Full Member として Public チームを見られるか(システム管理者を含む)
    pub fn is_full_member(&self) -> bool {
        matches!(self.role, Role::FullMember | Role::SystemAdmin)
    }

    pub fn is_system_admin(&self) -> bool {
        self.role == Role::SystemAdmin
    }

    /// チーム全体の所属
    pub fn team_membership(&self, team_id: i32) -> Option<&Membership> {
        self.memberships
            .iter()
            .find(|m| m.team_id == team_id && m.kind == MembershipKind::Team)
    }

    /// プロジェクト単位の所属があるか(同じチーム・同じプロジェクト)
    pub fn has_project_membership(&self, team_id: i32, project_id: i32) -> bool {
        self.memberships
            .iter()
            .any(|m| m.team_id == team_id && m.kind == (MembershipKind::Project { project_id }))
    }

    /// プロジェクト単位の所属があるか(チームを問わない)
    pub fn has_any_project_membership(&self, project_id: i32) -> bool {
        self.memberships
            .iter()
            .any(|m| m.kind == (MembershipKind::Project { project_id }))
    }

    /// 見えるチーム(チーム全体の所属 ∪ Full Member なら Public チーム)。重複なし・昇順
    pub fn visible_team_ids(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self
            .memberships
            .iter()
            .filter(|m| m.kind == MembershipKind::Team)
            .map(|m| m.team_id)
            .collect();
        if self.is_full_member() {
            ids.extend(self.public_team_ids.iter().copied());
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// プロジェクト単位の所属の (team_id, project_id)。重複なし・昇順
    pub fn project_grants(&self) -> Vec<(i32, i32)> {
        let mut v: Vec<(i32, i32)> = self
            .memberships
            .iter()
            .filter_map(|m| match m.kind {
                MembershipKind::Project { project_id } => Some((m.team_id, project_id)),
                MembershipKind::Team => None,
            })
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// 閲覧(Read)の判定に使う、チームの情報。公開区分は閲覧者が持つ Public チームの一覧から決める
    /// (DB を読まない)。設定の方針・Owner の数は閲覧の判定に関係しないので、既定値にしている。
    /// **管理(ManageSettings 等)の判定には使わない**(facts_repo で読むこと)。
    pub fn team_facts_for_read(&self, team_id: i32) -> super::resource::TeamFacts {
        use super::resource::{SettingsPolicy, TeamFacts, Visibility};
        TeamFacts {
            team_id,
            visibility: if self.public_team_ids.contains(&team_id) {
                Visibility::Public
            } else {
                Visibility::Private
            },
            settings_policy: SettingsPolicy::Members,
            owner_count: 0,
        }
    }
}
