//! 判定に必要な、リソースの所属の情報(本文は持たない)
//!
//! 本文を読む前に、これを小さな問い合わせで作って判定する(見えない物の本文を読まない)。

/// チームの公開区分
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

impl Visibility {
    /// DB の値(`m_team.visibility`)から。想定外の値は安全側(Private)に倒す
    pub fn from_db(s: &str) -> Self {
        if s == "public" {
            Visibility::Public
        } else {
            Visibility::Private
        }
    }
}

/// チームの設定を、誰が管理するか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPolicy {
    /// メンバー全員(既定)
    Members,
    /// Owner だけ
    Owners,
}

impl SettingsPolicy {
    /// DB の値(`m_team.settings_policy`)から。想定外の値は安全側(Owners)に倒す
    pub fn from_db(s: &str) -> Self {
        if s == "members" {
            SettingsPolicy::Members
        } else {
            SettingsPolicy::Owners
        }
    }
}

/// 判定に必要な、チームの情報
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamFacts {
    pub team_id: i32,
    pub visibility: Visibility,
    pub settings_policy: SettingsPolicy,
    /// 有効な Owner の数(Owner に絞ったチームで、Owner が 0 人なら全員に戻す判定に使う)
    pub owner_count: u32,
}

/// Wiki の所属
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikiScope {
    Team(TeamFacts),
    Project {
        project_id: i32,
        teams: Vec<TeamFacts>,
    },
    /// チームもプロジェクトも無い(全体の Wiki)
    Global,
}

/// 判定の対象
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceRef {
    Team(TeamFacts),
    Ticket {
        /// チームの無いチケットは None(作成者・担当者だけが見られる)
        team: Option<TeamFacts>,
        project_id: Option<i32>,
        author_id: Option<i32>,
        assignee_ids: Vec<i32>,
    },
    Project {
        project_id: i32,
        teams: Vec<TeamFacts>,
    },
    Cycle {
        team: TeamFacts,
    },
    Wiki {
        scope: WikiScope,
        author_id: Option<i32>,
    },
    /// カテゴリ・休日・全体のラベルなど(機密性の無い共通マスタ)
    GlobalMaster,
    /// ロードマップ(ワークスペース全体の物。設計書 §5.2。`owner_id` は責任の表示で、権限は持たない §4.5)
    Roadmap,
    /// ユーザー(一覧・プロフィール)
    User {
        user_id: i32,
        is_guest: bool,
        /// そのユーザーのチーム全体の所属
        team_ids: Vec<i32>,
        /// そのユーザーのプロジェクト単位の所属(team_id, project_id)
        project_grants: Vec<(i32, i32)>,
    },
}
