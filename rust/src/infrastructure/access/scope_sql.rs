//! scope_sql — 見える範囲(Scope)を、SQL の WHERE 句にする唯一の場所
//!
//! 規則は `domain::access::policy` の「閲覧(Read)」と一致させる。一致は、実 DB のテスト
//! (`parity_tests`)で、同じ閲覧者・同じ行について両方を比べて確かめる。ここだけを変えない。
//!
//! 使い方: `sqlx::QueryBuilder` の WHERE 句の途中で呼ぶ。条件は括弧で閉じて追加する。
//! ```ignore
//! let mut qb = QueryBuilder::new("SELECT t.id FROM tickets_ticket t WHERE ");
//! scope_sql::push_ticket_visible(&mut qb, "t", &scope);
//! ```

use sqlx::{Postgres, QueryBuilder};

use crate::domain::access::scope::Scope;

fn teams(scope: &Scope) -> Vec<i64> {
    scope.team_ids().iter().map(|&t| t as i64).collect()
}

fn grants(scope: &Scope) -> (Vec<i64>, Vec<i64>) {
    scope
        .project_grants()
        .iter()
        .map(|&(t, p)| (t as i64, p as i64))
        .unzip()
}

/// 別名(テーブルの別名)に、SQL として安全な文字だけが使われていること。
/// 呼び出し側が固定の文字列を渡す前提だが、誤って外部の値を渡したときに SQL を壊さないように確かめる。
fn alias(a: &str) -> &str {
    assert!(
        !a.is_empty()
            && a.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.'),
        "scope_sql: 不正な別名 {a:?}"
    );
    a
}

/// チームの列(`team_col`)が、見えるチームであること
pub fn push_team_visible(qb: &mut QueryBuilder<'_, Postgres>, team_col: &str, scope: &Scope) {
    qb.push("(")
        .push(alias(team_col))
        .push(" = ANY(")
        .push_bind(teams(scope))
        .push("::int8[]))");
}

/// チケット(別名 `t`)が見えること(設計書 §5.2)
/// - チームが見える
/// - またはプロジェクト単位の所属(同じチーム・同じプロジェクト)
/// - またはチームの無いチケットで、作成者か担当者
pub fn push_ticket_visible(qb: &mut QueryBuilder<'_, Postgres>, t: &str, scope: &Scope) {
    let t = alias(t);
    let (gt, gp) = grants(scope);
    qb.push("(")
        .push(t)
        .push(".team_id = ANY(")
        .push_bind(teams(scope))
        .push("::int8[]) OR (")
        .push(t)
        .push(".team_id, ")
        .push(t)
        .push(".project_id) IN (SELECT * FROM unnest(")
        .push_bind(gt)
        .push("::int8[], ")
        .push_bind(gp)
        .push("::int8[]))");
    match scope.user_id() {
        Some(uid) => {
            qb.push(" OR (")
                .push(t)
                .push(".team_id IS NULL AND (")
                .push(t)
                .push(".author_id = ")
                .push_bind(uid as i64)
                .push(" OR EXISTS (SELECT 1 FROM tickets_ticket_assignees a WHERE a.ticketmodel_id = ")
                .push(t)
                .push(".id AND a.user_id = ")
                .push_bind(uid as i64)
                // EXISTS( / AND ( / OR ( / 全体の ( を閉じる
                .push("))))");
        }
        None => {
            qb.push(")");
        }
    }
}

/// プロジェクト(`project_col` はプロジェクトの ID の列)が見えること
/// - 参加チームのどれかが見える
/// - またはプロジェクト単位の所属
pub fn push_project_visible(qb: &mut QueryBuilder<'_, Postgres>, project_col: &str, scope: &Scope) {
    let col = alias(project_col);
    let (_, gp) = grants(scope);
    qb.push("(EXISTS (SELECT 1 FROM tickets_project_teams pt WHERE pt.project_id = ")
        .push(col)
        .push(" AND pt.team_id = ANY(")
        .push_bind(teams(scope))
        .push("::int8[])) OR ")
        .push(col)
        .push(" = ANY(")
        .push_bind(gp)
        .push("::int8[]))");
}

/// サイクル(別名 `c`)が見えること(チームが見える。プロジェクト単位の所属では見えない)
pub fn push_cycle_visible(qb: &mut QueryBuilder<'_, Postgres>, c: &str, scope: &Scope) {
    push_team_visible(qb, &format!("{}.team_id", alias(c)), scope);
}

/// Wiki(別名 `w`)が見えること
/// - チームの Wiki: チームが見える
/// - プロジェクトの Wiki: 参加チームのどれかが見える(プロジェクト単位の所属では見えない)
/// - 全体の Wiki: Full Member
pub fn push_wiki_visible(qb: &mut QueryBuilder<'_, Postgres>, w: &str, scope: &Scope) {
    let w = alias(w);
    qb.push("(")
        .push(w)
        .push(".team_id = ANY(")
        .push_bind(teams(scope))
        .push("::int8[]) OR (")
        .push(w)
        .push(".project_id IS NOT NULL AND EXISTS (SELECT 1 FROM tickets_project_teams pt WHERE pt.project_id = ")
        .push(w)
        .push(".project_id AND pt.team_id = ANY(")
        .push_bind(teams(scope))
        .push("::int8[]))) OR (")
        .push(w)
        .push(".team_id IS NULL AND ")
        .push(w)
        .push(".project_id IS NULL AND ")
        .push_bind(scope.is_full_member())
        .push("))");
}

/// チーム・プロジェクト・全体のどれかに属する設定(別名 `x`。列 `team_id`・`project_id`)の、チームとプロジェクトの部分。
/// 全体の物(どちらも無い)の条件は、呼び出し側が `global` で足す。優先順位は `facts_repo::facts_for_scoped` と同じ
fn push_scoped_visible(
    qb: &mut QueryBuilder<'_, Postgres>,
    x: &str,
    scope: &Scope,
    global: impl FnOnce(&mut QueryBuilder<'_, Postgres>),
) {
    let x = alias(x);
    qb.push("((")
        .push(x)
        .push(".team_id IS NOT NULL AND ")
        .push(x)
        .push(".team_id = ANY(")
        .push_bind(teams(scope))
        .push("::int8[])) OR (")
        .push(x)
        .push(".team_id IS NULL AND ")
        .push(x)
        .push(".project_id IS NOT NULL AND ");
    push_project_visible(qb, &format!("{x}.project_id"), scope);
    qb.push(") OR (")
        .push(x)
        .push(".team_id IS NULL AND ")
        .push(x)
        .push(".project_id IS NULL AND ");
    global(qb);
    qb.push("))");
}

/// ラベル(別名 `l`)が見えること(D-4)
/// - チームのラベル: チームが見える
/// - プロジェクトのラベル(チームなし): プロジェクトが見える
/// - 全体のラベル: Full Member。Guest は、見えるチケットで使われている物だけ(詳細設計書 §7.4。
///   この規則は SQL だけにある。policy の GlobalMaster の Read は、Guest にも「可」)
pub fn push_label_visible(qb: &mut QueryBuilder<'_, Postgres>, l: &str, scope: &Scope) {
    let l = alias(l).to_string();
    push_scoped_visible(qb, &l, scope, |qb| {
        qb.push("(")
            .push_bind(scope.is_full_member())
            .push(" OR EXISTS (SELECT 1 FROM tickets_ticket_labels tl JOIN tickets_ticket lt ON lt.id = tl.ticketmodel_id WHERE tl.labelmodel_id = ")
            .push(l.clone())
            .push(".id AND ");
        push_ticket_visible(qb, "lt", scope);
        qb.push("))");
    });
}

/// ワークフローの状態(別名 `ws`)が見えること(D-4)。規則はラベルと同じ。
/// 全体の状態を Guest が見られるのは、見えるチケットで使われている(`status` が同じ `slug`)物だけ(§7.4)
pub fn push_workflow_status_visible(qb: &mut QueryBuilder<'_, Postgres>, ws: &str, scope: &Scope) {
    let ws = alias(ws).to_string();
    push_scoped_visible(qb, &ws, scope, |qb| {
        qb.push("(")
            .push_bind(scope.is_full_member())
            .push(" OR EXISTS (SELECT 1 FROM tickets_ticket st WHERE st.status = ")
            .push(ws.clone())
            .push(".slug AND ");
        push_ticket_visible(qb, "st", scope);
        qb.push("))");
    });
}

/// マイルストーン(別名 `m`)が見えること(D-4)
/// - プロジェクトのマイルストーン: プロジェクトが見える
/// - プロジェクトの無いマイルストーン: 全員(共通のマスタ)
pub fn push_milestone_visible(qb: &mut QueryBuilder<'_, Postgres>, m: &str, scope: &Scope) {
    let m = alias(m);
    qb.push("(").push(m).push(".project_id IS NULL OR ");
    push_project_visible(qb, &format!("{m}.project_id"), scope);
    qb.push(")");
}

/// チームの物か全体の物(列 `team_col` が NULL)が見えること(D-4。チームのルール)
/// - チームの物: チームが見える
/// - 全体の物: 全員(共通のマスタ)
pub fn push_team_or_global_visible(
    qb: &mut QueryBuilder<'_, Postgres>,
    team_col: &str,
    scope: &Scope,
) {
    let col = alias(team_col);
    qb.push("(").push(col).push(" IS NULL OR ");
    push_team_visible(qb, col, scope);
    qb.push(")");
}
