//! Scope — 閲覧者から作る「見える範囲」。一覧を読むリポジトリ関数の、必須の引数
//!
//! 閲覧者(Viewer)からしか作れない。リポジトリ関数は `&Scope` か `&SystemContext` を要求し、
//! どちらも無しにテナントのデータを読む関数を作らない(呼び忘れをコンパイルエラーにする。詳細設計書 §7)。

use super::viewer::Viewer;

#[derive(Debug, Clone)]
pub struct Scope {
    /// 見えるチーム(チーム全体の所属 ∪ Full Member なら Public チーム)
    team_ids: Vec<i32>,
    /// プロジェクト単位の所属の (team_id, project_id)
    project_grants: Vec<(i32, i32)>,
    /// 人のユーザー ID(チームの無いチケットの、作成者・担当者の判定に使う。外部連携は None)
    user_id: Option<i32>,
    /// Full Member か(全体の Wiki の判定)
    full_member: bool,
}

impl Viewer {
    /// この閲覧者の見える範囲
    pub fn scope(&self) -> Scope {
        Scope {
            team_ids: self.visible_team_ids(),
            project_grants: self.project_grants(),
            user_id: self.user_id(),
            full_member: self.is_full_member(),
        }
    }
}

impl Scope {
    pub fn team_ids(&self) -> &[i32] {
        &self.team_ids
    }
    pub fn project_grants(&self) -> &[(i32, i32)] {
        &self.project_grants
    }
    pub fn user_id(&self) -> Option<i32> {
        self.user_id
    }
    pub fn is_full_member(&self) -> bool {
        self.full_member
    }
}
