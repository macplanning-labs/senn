//! infrastructure/repositories/kb_repo.rs — KB エンジン用データ取得(読み取り専用)
//!
//! クローズ判定に必要な情報(対象チケットと全子孫、ラベル、担当者数、本文)を集める。

use crate::domain::services::kb::close_check::{CloseCheckInput, TicketRow};
use crate::infrastructure::repositories::insights_repo::category_sql;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

/// 対象 + 全子孫の上限(これを超えるときは判定せずエラーにする。誤判定より失敗を選ぶ)
const MAX_TICKETS: i64 = 500;

/// 「おわった」とみなすワークフロー分類。キャンセルされたサブタスクは完了を妨げない。
const DONE_CATEGORIES_SQL: &str = "('completed', 'cancelled')";

/// 対象チケットと全子孫からクローズ判定の入力を作る。
///
/// - 対象が無い: `Ok(None)`
/// - 対象 + 子孫が 501 件以上: `Err("too many descendants")`
/// - 完了の判定は、チケットが属するプロジェクト/チームのワークフロー設定(カテゴリ)で DB 側が決める。
///   分類を引けないステータスは「未完了」として扱う(見落として「可」と言うより、止める側に倒す)。
pub async fn load_close_check_input(
    pool: &PgPool,
    ticket_key: &str,
) -> anyhow::Result<Option<CloseCheckInput>> {
    load_close_check_input_with_limit(pool, ticket_key, MAX_TICKETS).await
}

/// 上の本体。上限を引数で受ける(テストで少ない件数のまま、上限の挙動を確かめるため。
/// 大量のチケットを一度に作ると、DB の通知(リアルタイム通信)が大量に出て、
/// 同時に走る別のテストを乱すため)
async fn load_close_check_input_with_limit(
    pool: &PgPool,
    ticket_key: &str,
    max_tickets: i64,
) -> anyhow::Result<Option<CloseCheckInput>> {
    let target = sqlx::query(
        "SELECT id::int8 AS id, ticket_key, description, (parent_id IS NOT NULL) AS has_parent
         FROM tickets_ticket WHERE ticket_key = $1",
    )
    .bind(ticket_key)
    .fetch_optional(pool)
    .await?;
    let Some(target) = target else {
        return Ok(None);
    };
    let target_id: i64 = target.try_get("id")?;
    let target_key: String = target.try_get("ticket_key")?;
    let target_description: String = target.try_get("description")?;
    let target_is_subticket: bool = target.try_get("has_parent")?;

    // 対象 + 全子孫。UNION なので、壊れた循環データでも再帰が止まる。
    // category_sql は定数の式を返す(ユーザー入力を含まない)。値はすべて bind。
    let cat = category_sql("t.status");
    let sql = format!(
        "WITH RECURSIVE d(id) AS (
            SELECT id FROM tickets_ticket WHERE id = $1
            UNION
            SELECT c.id FROM tickets_ticket c JOIN d ON c.parent_id = d.id
         )
         SELECT t.id::int8 AS id, t.ticket_key, t.ticket_type,
                t.parent_id::int8 AS parent_id,
                (COALESCE({cat}, '') IN {DONE_CATEGORIES_SQL}) AS done
         FROM tickets_ticket t JOIN d ON d.id = t.id
         LIMIT $2"
    );
    let rows = sqlx::query(&sql)
        .bind(target_id)
        .bind(max_tickets + 1)
        .fetch_all(pool)
        .await?;
    if rows.len() as i64 > max_tickets {
        return Err(anyhow::anyhow!("too many descendants"));
    }

    let mut key_by_id: HashMap<i64, String> = HashMap::with_capacity(rows.len());
    for r in &rows {
        key_by_id.insert(r.try_get("id")?, r.try_get("ticket_key")?);
    }
    let mut tickets = Vec::with_capacity(rows.len());
    for r in &rows {
        // 読み取りに失敗したら必ずエラーにする(親が「なし」に化けると、未完了の子を見落とす)
        let parent_id: Option<i64> = r.try_get("parent_id")?;
        tickets.push(TicketRow {
            key: r.try_get("ticket_key")?,
            ticket_type: r.try_get("ticket_type")?,
            done: r.try_get("done")?,
            parent_key: parent_id.and_then(|pid| key_by_id.get(&pid).cloned()),
        });
    }

    let target_labels: Vec<String> = sqlx::query_scalar(
        "SELECT m.name FROM tickets_ticket_labels tl
         JOIN m_label m ON tl.labelmodel_id = m.id
         WHERE tl.ticketmodel_id = $1 ORDER BY m.name",
    )
    .bind(target_id)
    .fetch_all(pool)
    .await?;

    let target_assignee_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::int8 FROM tickets_ticket_assignees WHERE ticketmodel_id = $1",
    )
    .bind(target_id)
    .fetch_one(pool)
    .await?;

    Ok(Some(CloseCheckInput {
        target: target_key,
        tickets,
        target_labels,
        target_description,
        target_assignee_count,
        target_is_subticket,
    }))
}

/// 対象チケットのステータス分類を `completed` / `cancelled` / `started` / `other` に正規化して返す。
/// 分類はチームのワークフロー設定で決まる(`category_sql`)。引けないものは `other`。
/// チケットが無いときは `Ok(None)`。
pub async fn load_status_category(
    pool: &PgPool,
    ticket_key: &str,
) -> anyhow::Result<Option<String>> {
    let cat = category_sql("t.status");
    let sql = format!("SELECT COALESCE({cat}, '') FROM tickets_ticket t WHERE t.ticket_key = $1");
    let raw: Option<String> = sqlx::query_scalar(&sql)
        .bind(ticket_key)
        .fetch_optional(pool)
        .await?;
    Ok(raw.map(|c| normalize_category(&c)))
}

fn normalize_category(raw: &str) -> String {
    match raw {
        "completed" => "completed",
        "cancelled" | "canceled" => "cancelled",
        "started" => "started",
        _ => "other",
    }
    .to_string()
}

/// 対象チケットのコメント件数(削除済みを除く)
pub async fn count_comments(pool: &PgPool, ticket_key: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*)::int8 FROM tickets_comment c
         JOIN tickets_ticket t ON t.id = c.ticket_id
         WHERE t.ticket_key = $1 AND c.deleted_at IS NULL",
    )
    .bind(ticket_key)
    .fetch_one(pool)
    .await?)
}

// ───────────── 2B: 似た完了済みの機能追加(成功事例)の取得 ─────────────

/// 成功事例の候補(同じプロジェクトの、完了済み・障害でないチケット)
#[derive(Debug, Clone, PartialEq)]
pub struct PatternCandidate {
    pub key: String,
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    /// タイトル・本文・コメントのテキスト(ファイルパスの検出用)。節の抽出には description を使う
    pub text_for_files: String,
    /// 一致した検索語のうち、互いに重ならないもの(一致の理由の表示用)
    pub matched_terms: Vec<String>,
    /// 一致の点数(= 重ならない一致語の数)
    pub score: usize,
}

/// 障害とみなす種別/ラベル(これらは成功事例の対象外。障害の知見は第3弾)
const BUG_TYPES_SQL: &str = "('bug', '不具合')";
const BUG_LABELS_SQL: &str = "('type:incident', 'type:infra')";

/// LIKE の特別な文字(`%` `_` `\`)をエスケープして、部分一致の検索語にする
pub(crate) fn like_pattern(term: &str) -> String {
    let mut o = String::with_capacity(term.len() + 2);
    o.push('%');
    for c in term.chars() {
        if matches!(c, '%' | '_' | '\\') {
            o.push('\\');
        }
        o.push(c);
    }
    o.push('%');
    o
}

/// 対象が「障害でない」か(障害の知見は第3弾なので、2B は障害のときは何も添えない)
pub async fn is_not_incident(pool: &PgPool, ticket_key: &str) -> anyhow::Result<Option<bool>> {
    let sql = format!(
        "SELECT NOT (t.ticket_type IN {BUG_TYPES_SQL}
                     OR EXISTS (SELECT 1 FROM tickets_ticket_labels tl JOIN m_label m ON m.id = tl.labelmodel_id
                                WHERE tl.ticketmodel_id = t.id AND m.name IN {BUG_LABELS_SQL}))
         FROM tickets_ticket t WHERE t.ticket_key = $1"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(ticket_key)
        .fetch_optional(pool)
        .await?)
}

/// 候補にするチケットの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateKind {
    /// 障害でないもの(成功事例の知見・2B)
    Feature,
    /// 障害(bug または type:incident|infra)(類似障害の知見・3A)
    Incident,
}

/// ありふれた語を判断するのに必要な、比べる相手(完了済みの機能追加など)の最低件数
const COMMON_TERM_MIN_BASE: i64 = 8;

/// 似た成功事例の候補を、一致した語の数が多い順に最大 `limit` 件返す。
///
/// - 同じプロジェクトの、完了(カテゴリ completed)・障害でない・対象自身ではないチケットだけ。
/// - タイトル・本文のどちらかに、検索語が含まれた「異なる語の数」を数える。`min_hits` 未満は除く。
/// - 一致の数は、互いに重ならない検索語だけを数える(同じ言い回しの窓が 2 つ一致しても 1 件)。
/// - 比べる相手が COMMON_TERM_MIN_BASE 件以上あるとき、相手の 1/4 より多くに出る「ありふれた語」は数えない。
/// - 検索語は bind(LIKE の特別な文字はエスケープ)。SQL に値を連結しない。
pub async fn find_pattern_candidates(
    pool: &PgPool,
    ticket_key: &str,
    kind: CandidateKind,
    terms: &[String],
    min_hits: i64,
    limit: i64,
) -> anyhow::Result<Vec<PatternCandidate>> {
    find_candidates_scoped(pool, Some(ticket_key), None, kind, terms, min_hits, limit).await
}

/// プロジェクトを直接指定して候補を探す(まだチケットが無い新規起票用)。
/// `exclude_key` があれば、そのチケットは除く。ほかの条件は `find_pattern_candidates` と同じ。
pub async fn find_candidates_in_project(
    pool: &PgPool,
    project_id: i32,
    exclude_key: Option<&str>,
    kind: CandidateKind,
    terms: &[String],
    min_hits: i64,
    limit: i64,
) -> anyhow::Result<Vec<PatternCandidate>> {
    find_candidates_scoped(
        pool,
        exclude_key,
        Some(project_id),
        kind,
        terms,
        min_hits,
        limit,
    )
    .await
}

/// 本体。プロジェクトは「`ticket_key` のチケットのプロジェクト」、無ければ `project_id`。
/// どちらも決まらなければ、候補は無い(空)。
async fn find_candidates_scoped(
    pool: &PgPool,
    ticket_key: Option<&str>,
    project_id: Option<i32>,
    kind: CandidateKind,
    terms: &[String],
    min_hits: i64,
    limit: i64,
) -> anyhow::Result<Vec<PatternCandidate>> {
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    let patterns: Vec<String> = terms.iter().map(|t| like_pattern(t)).collect();
    let cat = category_sql("t.status");
    // 1. base: 同じプロジェクトの、完了・障害でない・自分以外のチケット
    // 2. pats: ありふれた語(base の 1/4 より多くに出る語)を除く。「機能を追加する」のような
    //    どのチケットにも出る語だけで一致して、関係の薄い事例が選ばれるのを防ぐ。
    //    base が少ない(COMMON_TERM_MIN_BASE 件未満)プロジェクトでは、ありふれたかを判断できないので除かない。
    // 3. 残った語ごとに「タイトルか本文に含まれるか」を 0/1 にして足す(一致した異なる語の数)
    let is_incident = format!(
        "(t.ticket_type IN {BUG_TYPES_SQL}
               OR EXISTS (SELECT 1 FROM tickets_ticket_labels tl JOIN m_label m ON m.id = tl.labelmodel_id
                           WHERE tl.ticketmodel_id = t.id AND m.name IN {BUG_LABELS_SQL}))"
    );
    let kind_cond = match kind {
        CandidateKind::Incident => is_incident,
        CandidateKind::Feature => format!("NOT {is_incident}"),
    };
    let sql = format!(
        "WITH me AS (SELECT COALESCE((SELECT id FROM tickets_ticket WHERE ticket_key = $1), 0) AS id,
                            COALESCE((SELECT project_id FROM tickets_ticket WHERE ticket_key = $1), $5::int) AS project_id),
         base AS (
            SELECT t.id, t.ticket_key, t.title, t.description, t.updated_at
              FROM tickets_ticket t
              JOIN me ON t.project_id = me.project_id AND t.id <> me.id
             WHERE COALESCE({cat}, '') = 'completed'
               AND {kind_cond}
         ),
         n AS (SELECT COUNT(*) AS c FROM base),
         pats AS (
            SELECT p.pat, p.ord FROM unnest($2::text[]) WITH ORDINALITY AS p(pat, ord), n
             WHERE n.c < {COMMON_TERM_MIN_BASE}
                OR (SELECT COUNT(*) FROM base b WHERE b.title ILIKE p.pat OR b.description ILIKE p.pat) * 4 <= n.c
         ),
         scored AS (
            SELECT b.*,
                   (SELECT COUNT(*) FROM pats p WHERE b.title ILIKE p.pat OR b.description ILIKE p.pat) AS hits,
                   (SELECT COALESCE(array_agg(p.ord::int ORDER BY p.ord), ARRAY[]::int[])
                      FROM pats p WHERE b.title ILIKE p.pat OR b.description ILIKE p.pat) AS matched
              FROM base b
         )
         SELECT t.ticket_key, t.title, t.description,
                (SELECT COALESCE(array_agg(m.name ORDER BY m.name), ARRAY[]::text[])
                   FROM tickets_ticket_labels tl JOIN m_label m ON m.id = tl.labelmodel_id
                  WHERE tl.ticketmodel_id = t.id) AS labels,
                COALESCE((SELECT string_agg(c.body, E'\n') FROM tickets_comment c
                           WHERE c.ticket_id = t.id AND c.deleted_at IS NULL), '') AS comments,
                t.matched
           FROM scored t
          WHERE t.hits >= $3
          ORDER BY t.hits DESC, t.updated_at DESC, t.id DESC
          LIMIT $4"
    );
    let rows = sqlx::query(&sql)
        .bind(ticket_key)
        .bind(&patterns)
        .bind(min_hits)
        .bind(limit.saturating_mul(OVERFETCH))
        .bind(project_id)
        .fetch_all(pool)
        .await?;
    // 重なる検索語(同じ言い回しの窓)は 1 件として数え直し、独立した一致が min_hits 未満なら除く。
    // SQL の並び(一致が多い順)を保ったまま、独立した一致の多い順に整える。
    let mut scored: Vec<(usize, PatternCandidate)> = Vec::with_capacity(rows.len());
    for r in &rows {
        let matched: Vec<i32> = r.try_get("matched")?;
        let hit_terms: Vec<&str> = matched
            .iter()
            .filter_map(|o| usize::try_from(*o).ok().and_then(|o| o.checked_sub(1)))
            .filter_map(|i| terms.get(i).map(String::as_str))
            .collect();
        let independent_terms = independent_terms(&hit_terms);
        let independent = independent_terms.len();
        if (independent as i64) < min_hits {
            continue;
        }
        let title: String = r.try_get("title")?;
        let description: String = r.try_get("description")?;
        let comments: String = r.try_get("comments")?;
        scored.push((
            independent,
            PatternCandidate {
                key: r.try_get("ticket_key")?,
                text_for_files: format!("{title}\n{description}\n{comments}"),
                title,
                description,
                labels: r.try_get("labels")?,
                matched_terms: independent_terms.iter().map(|t| t.to_string()).collect(),
                score: independent,
            },
        ));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0)); // 安定ソート: 同数なら SQL の並びのまま
    Ok(scored
        .into_iter()
        .take(usize::try_from(limit).unwrap_or(0))
        .map(|(_, c)| c)
        .collect())
}

/// 類似障害の検索(文字の重なり方式)で、点数をつける対象を読む上限(新しい順)
pub const INCIDENT_POOL_MAX: i64 = 1_000;

/// プロジェクトの、完了済みの障害(bug/不具合、または type:incident|infra)を、新しい順に最大 `limit` 件読む。
/// `exclude_key` があれば、そのチケットは除く。語の絞り込みはしない(点数づけは呼び出し側)。
/// 返す `matched_terms` は空、`score` は 0。
pub async fn list_project_incidents(
    pool: &PgPool,
    project_id: i32,
    exclude_key: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<PatternCandidate>> {
    let cat = category_sql("t.status");
    let sql = format!(
        "SELECT t.ticket_key, t.title, t.description,
                (SELECT COALESCE(array_agg(m.name ORDER BY m.name), ARRAY[]::text[])
                   FROM tickets_ticket_labels tl JOIN m_label m ON m.id = tl.labelmodel_id
                  WHERE tl.ticketmodel_id = t.id) AS labels,
                COALESCE((SELECT string_agg(c.body, E'\n') FROM tickets_comment c
                           WHERE c.ticket_id = t.id AND c.deleted_at IS NULL), '') AS comments
           FROM tickets_ticket t
          WHERE t.project_id = $1
            AND ($2::text IS NULL OR t.ticket_key <> $2)
            AND COALESCE({cat}, '') = 'completed'
            AND (t.ticket_type IN {BUG_TYPES_SQL}
                 OR EXISTS (SELECT 1 FROM tickets_ticket_labels tl JOIN m_label m ON m.id = tl.labelmodel_id
                             WHERE tl.ticketmodel_id = t.id AND m.name IN {BUG_LABELS_SQL}))
          ORDER BY t.updated_at DESC, t.id DESC
          LIMIT $3"
    );
    let rows = sqlx::query(&sql)
        .bind(project_id)
        .bind(exclude_key)
        .bind(limit)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let title: String = r.try_get("title")?;
        let description: String = r.try_get("description")?;
        let comments: String = r.try_get("comments")?;
        out.push(PatternCandidate {
            key: r.try_get("ticket_key")?,
            text_for_files: format!("{title}\n{description}\n{comments}"),
            title,
            description,
            labels: r.try_get("labels")?,
            matched_terms: Vec::new(),
            score: 0,
        });
    }
    Ok(out)
}

/// SQL で多めに取る倍率(重なりを数え直して減ったあとでも、上限件数を満たすため)
const OVERFETCH: i64 = 4;

/// 2 つの検索語が重なるか(一方がもう一方を含む、または端が 2 文字以上つながる)。
/// 長い語を 6 文字の窓に切って作った検索語は、同じ言い回しで隣の窓と重なる。
fn terms_overlap(a: &str, b: &str) -> bool {
    if a.contains(b) || b.contains(a) {
        return true;
    }
    let edge = |x: &str, y: &str| {
        let (xc, yc): (Vec<char>, Vec<char>) = (x.chars().collect(), y.chars().collect());
        (2..=xc.len().min(yc.len())).any(|k| xc[xc.len() - k..] == yc[..k])
    };
    edge(a, b) || edge(b, a)
}

/// 一致した検索語のうち、互いに重ならないもの(先に出たものを優先して残す)。
fn independent_terms<'a>(hit_terms: &[&'a str]) -> Vec<&'a str> {
    let mut chosen: Vec<&str> = Vec::new();
    for t in hit_terms {
        if chosen.iter().all(|c| !terms_overlap(c, t)) {
            chosen.push(t);
        }
    }
    chosen
}

/// 互いに重ならない一致語の数。
#[cfg(test)]
fn independent_hits(hit_terms: &[&str]) -> usize {
    independent_terms(hit_terms).len()
}

/// `feature_key` のあとに、その番号を本文・コメントに書いて起きた障害チケットのキー(最大 `limit` 件)。
/// 同じプロジェクトの障害だけ。
pub async fn find_followup_bugs(
    pool: &PgPool,
    feature_key: &str,
    limit: i64,
) -> anyhow::Result<Vec<String>> {
    let pat = like_pattern(feature_key);
    let sql = format!(
        "SELECT b.ticket_key
         FROM tickets_ticket f
         JOIN tickets_ticket b ON b.project_id = f.project_id AND b.id <> f.id
         WHERE f.ticket_key = $1
           AND (b.ticket_type IN {BUG_TYPES_SQL}
                OR EXISTS (SELECT 1 FROM tickets_ticket_labels tl JOIN m_label m ON m.id = tl.labelmodel_id
                            WHERE tl.ticketmodel_id = b.id AND m.name IN {BUG_LABELS_SQL}))
           AND (b.title ILIKE $2 OR b.description ILIKE $2
                OR EXISTS (SELECT 1 FROM tickets_comment c WHERE c.ticket_id = b.id
                            AND c.deleted_at IS NULL AND c.body ILIKE $2))
         ORDER BY b.id
         LIMIT $3"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(feature_key)
        .bind(pat)
        .bind(limit)
        .fetch_all(pool)
        .await?)
}

/// キーからタイトルを引く(表示用)。無ければ None
pub async fn title_of(pool: &PgPool, ticket_key: &str) -> anyhow::Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT title FROM tickets_ticket WHERE ticket_key = $1")
            .bind(ticket_key)
            .fetch_optional(pool)
            .await?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        create_test_project, create_test_ticket, create_test_user, test_pool, unique_suffix,
    };

    async fn key_of(pool: &PgPool, id: i32) -> String {
        sqlx::query_scalar("SELECT ticket_key FROM tickets_ticket WHERE id = $1")
            .bind(id)
            .fetch_one(pool)
            .await
            .expect("key")
    }

    async fn set(pool: &PgPool, id: i32, parent: Option<i32>, status: &str) {
        sqlx::query("UPDATE tickets_ticket SET parent_id = $2, status = $3 WHERE id = $1")
            .bind(id)
            .bind(parent)
            .bind(status)
            .execute(pool)
            .await
            .expect("update");
    }

    fn row<'a>(input: &'a CloseCheckInput, key: &str) -> &'a TicketRow {
        input
            .tickets
            .iter()
            .find(|t| t.key == key)
            .unwrap_or_else(|| panic!("ticket {key} not loaded"))
    }

    #[tokio::test]
    async fn unknown_key_is_none() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let r = load_close_check_input(&pool, "NOPE-999999").await;
        assert!(matches!(r, Ok(None)));
    }

    #[tokio::test]
    async fn loads_descendants_done_flags_labels_and_assignees() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbrepo").await;
        let project = create_test_project(&pool, "KBR", user).await;

        let p = create_test_ticket(&pool, project, "KBR", user).await;
        let c_done = create_test_ticket(&pool, project, "KBR", user).await;
        let c_open = create_test_ticket(&pool, project, "KBR", user).await;
        let c_cancel = create_test_ticket(&pool, project, "KBR", user).await;
        let c_custom = create_test_ticket(&pool, project, "KBR", user).await;
        let g_open = create_test_ticket(&pool, project, "KBR", user).await;
        let unrelated = create_test_ticket(&pool, project, "KBR", user).await;

        // このプロジェクト独自のステータス shipped(カテゴリ completed)を作る
        sqlx::query(
            "INSERT INTO t_workflow_status (name, slug, category, color, position, is_default, project_id)
             VALUES ('Shipped', 'shipped', 'completed', '#10b981', 10, false, $1)",
        )
        .bind(project)
        .execute(&pool)
        .await
        .expect("workflow status");

        set(&pool, p, None, "in_progress").await;
        set(&pool, c_done, Some(p), "closed").await;
        set(&pool, c_open, Some(p), "in_progress").await;
        set(&pool, c_cancel, Some(p), "cancelled").await;
        set(&pool, c_custom, Some(p), "shipped").await;
        set(&pool, g_open, Some(c_done), "open").await;
        set(&pool, unrelated, None, "open").await;

        sqlx::query("UPDATE tickets_ticket SET description = $2 WHERE id = $1")
            .bind(p)
            .bind("## まとめ\n本文です")
            .execute(&pool)
            .await
            .expect("desc");
        let label_id: i64 = sqlx::query_scalar(
            "INSERT INTO m_label (name, color, created_at, project_id)
             VALUES ($1, '#000000', NOW(), $2) RETURNING id::int8",
        )
        .bind(format!("type:feature-{}", unique_suffix()))
        .bind(project)
        .fetch_one(&pool)
        .await
        .expect("label");
        sqlx::query(
            "INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)",
        )
        .bind(p)
        .bind(label_id)
        .execute(&pool)
        .await
        .expect("link label");
        sqlx::query(
            "INSERT INTO tickets_ticket_assignees (ticketmodel_id, user_id) VALUES ($1, $2)",
        )
        .bind(p)
        .bind(user)
        .execute(&pool)
        .await
        .expect("assignee");

        let pk = key_of(&pool, p).await;
        let input = load_close_check_input(&pool, &pk)
            .await
            .expect("ok")
            .expect("some");

        assert_eq!(input.target, pk);
        assert_eq!(
            input.tickets.len(),
            6,
            "対象 + 子4 + 孫1(無関係チケットは含めない)"
        );
        let unrelated_key = key_of(&pool, unrelated).await;
        assert!(!input.tickets.iter().any(|t| t.key == unrelated_key));

        assert!(!row(&input, &pk).done);
        assert_eq!(row(&input, &pk).parent_key, None);
        let ck = key_of(&pool, c_done).await;
        assert!(row(&input, &ck).done, "closed は完了");
        assert_eq!(row(&input, &ck).parent_key.as_deref(), Some(pk.as_str()));
        assert!(!row(&input, &key_of(&pool, c_open).await).done);
        assert!(
            row(&input, &key_of(&pool, c_cancel).await).done,
            "キャンセルは妨げない"
        );
        assert!(
            row(&input, &key_of(&pool, c_custom).await).done,
            "独自ステータスもワークフローのカテゴリで完了と判定"
        );
        let gk = key_of(&pool, g_open).await;
        assert!(!row(&input, &gk).done);
        assert_eq!(row(&input, &gk).parent_key.as_deref(), Some(ck.as_str()));

        assert_eq!(input.target_assignee_count, 1);
        assert!(
            !input.target_is_subticket,
            "親(トップレベル)はサブチケットではない"
        );
        assert_eq!(input.target_labels.len(), 1);
        assert!(input.target_labels[0].starts_with("type:feature-"));
        assert_eq!(input.target_description, "## まとめ\n本文です");
    }

    #[tokio::test]
    async fn unknown_status_is_treated_as_not_done() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbrepo").await;
        let project = create_test_project(&pool, "KBU", user).await;
        let p = create_test_ticket(&pool, project, "KBU", user).await;
        let c = create_test_ticket(&pool, project, "KBU", user).await;
        set(&pool, p, None, "open").await;
        set(&pool, c, Some(p), "weird_status").await;
        let input = load_close_check_input(&pool, &key_of(&pool, p).await)
            .await
            .expect("ok")
            .expect("some");
        assert!(!row(&input, &key_of(&pool, c).await).done);
    }

    #[tokio::test]
    async fn cyclic_parents_terminate() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbrepo").await;
        let project = create_test_project(&pool, "KBC", user).await;
        let a = create_test_ticket(&pool, project, "KBC", user).await;
        let b = create_test_ticket(&pool, project, "KBC", user).await;
        set(&pool, a, Some(b), "open").await;
        set(&pool, b, Some(a), "open").await;
        let input = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            load_close_check_input(&pool, &key_of(&pool, a).await),
        )
        .await
        .expect("must not hang")
        .expect("ok")
        .expect("some");
        assert_eq!(input.tickets.len(), 2);
    }

    /// 上限(本番は 500)を超えるとエラー、ちょうどなら通る。少ない件数で確かめる
    #[tokio::test]
    async fn too_many_descendants_is_an_error_and_exactly_the_limit_is_fine() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbrepo").await;
        let project = create_test_project(&pool, "KBM", user).await;
        let p = create_test_ticket(&pool, project, "KBM", user).await;
        let c1 = create_test_ticket(&pool, project, "KBM", user).await;
        let c2 = create_test_ticket(&pool, project, "KBM", user).await;
        set(&pool, c1, Some(p), "open").await;
        set(&pool, c2, Some(p), "open").await;
        let key = key_of(&pool, p).await;

        // 対象 + 子2件 = 3件
        let ok = load_close_check_input_with_limit(&pool, &key, 3).await;
        assert_eq!(
            ok.expect("exactly the limit").expect("some").tickets.len(),
            3
        );

        let err = load_close_check_input_with_limit(&pool, &key, 2)
            .await
            .expect_err("over the limit must fail");
        assert!(err.to_string().contains("too many descendants"), "{err}");

        // 本番の上限は 500 のまま
        assert_eq!(MAX_TICKETS, 500);
    }

    #[test]
    fn category_is_normalized_to_a_closed_set() {
        for (raw, want) in [
            ("completed", "completed"),
            ("cancelled", "cancelled"),
            ("canceled", "cancelled"),
            ("started", "started"),
            ("unstarted", "other"),
            ("", "other"),
            ("'; DROP TABLE x; --", "other"),
        ] {
            assert_eq!(normalize_category(raw), want, "{raw}");
        }
    }

    #[tokio::test]
    async fn status_category_and_comment_count_are_loaded() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbcat").await;
        let project = create_test_project(&pool, "KBG", user).await;
        let t = create_test_ticket(&pool, project, "KBG", user).await;
        let key = key_of(&pool, t).await;

        set(&pool, t, None, "in_progress").await;
        assert_eq!(
            load_status_category(&pool, &key)
                .await
                .expect("ok")
                .as_deref(),
            Some("started")
        );
        set(&pool, t, None, "closed").await;
        assert_eq!(
            load_status_category(&pool, &key)
                .await
                .expect("ok")
                .as_deref(),
            Some("completed")
        );
        set(&pool, t, None, "open").await;
        assert_eq!(
            load_status_category(&pool, &key)
                .await
                .expect("ok")
                .as_deref(),
            Some("other")
        );
        assert_eq!(
            load_status_category(&pool, "NOPE-1").await.expect("ok"),
            None
        );

        assert_eq!(count_comments(&pool, &key).await.expect("ok"), 0);
        for (body, deleted) in [("a", false), ("b", false), ("c", true)] {
            sqlx::query(
                "INSERT INTO tickets_comment (body, created_at, author_id, ticket_id, deleted_at)
                 VALUES ($1, NOW(), $2, $3, CASE WHEN $4 THEN NOW() END)",
            )
            .bind(body)
            .bind(user as i64)
            .bind(t as i64)
            .bind(deleted)
            .execute(&pool)
            .await
            .expect("comment");
        }
        assert_eq!(
            count_comments(&pool, &key).await.expect("ok"),
            2,
            "削除済みは数えない"
        );
    }

    #[tokio::test]
    async fn child_ticket_is_flagged_as_subticket() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbsub").await;
        let project = create_test_project(&pool, "KBU2", user).await;
        let p = create_test_ticket(&pool, project, "KBU2", user).await;
        let c = create_test_ticket(&pool, project, "KBU2", user).await;
        set(&pool, c, Some(p), "open").await;

        let child = load_close_check_input(&pool, &key_of(&pool, c).await)
            .await
            .expect("ok")
            .expect("some");
        assert!(child.target_is_subticket, "親を持つ子はサブチケット");
        // 親は、子の取得対象に含まれない(判定の対象は、その子と子孫だけ)
        assert_eq!(child.tickets.len(), 1);

        let parent = load_close_check_input(&pool, &key_of(&pool, p).await)
            .await
            .expect("ok")
            .expect("some");
        assert!(!parent.target_is_subticket);
    }

    // ───────────── 2B: 成功事例の候補 ─────────────

    #[test]
    fn like_pattern_escapes_special_characters() {
        assert_eq!(like_pattern("abc"), "%abc%");
        assert_eq!(like_pattern("100%"), "%100\\%%");
        assert_eq!(like_pattern("a_b"), "%a\\_b%");
        assert_eq!(like_pattern("a\\b"), "%a\\\\b%");
        assert_eq!(like_pattern(""), "%%");
    }

    async fn make(
        pool: &PgPool,
        project: i32,
        user: i32,
        title: &str,
        desc: &str,
        status: &str,
        ty: &str,
    ) -> i32 {
        let id = create_test_ticket(pool, project, "KBP", user).await;
        sqlx::query("UPDATE tickets_ticket SET title = $2, description = $3, status = $4, ticket_type = $5 WHERE id = $1")
            .bind(id)
            .bind(title)
            .bind(desc)
            .bind(status)
            .bind(ty)
            .execute(pool)
            .await
            .expect("update");
        id
    }

    fn terms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn candidates_are_same_project_completed_non_bug_and_not_self() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let other_project = create_test_project(&pool, "KBQ", user).await;
        let me = make(
            &pool,
            project,
            user,
            "請求書の一括出力を作る",
            "請求書 PDF 一括出力",
            "open",
            "task",
        )
        .await;
        let ok1 = make(
            &pool,
            project,
            user,
            "請求書PDFの出力",
            "請求書 一括出力 を実装",
            "closed",
            "task",
        )
        .await;
        let ok2 = make(
            &pool,
            project,
            user,
            "請求書の一括出力",
            "一括出力",
            "resolved",
            "issue",
        )
        .await;
        // 除外されるべきもの
        let open_one = make(
            &pool,
            project,
            user,
            "請求書の一括出力(未完了)",
            "請求書 一括出力",
            "in_progress",
            "task",
        )
        .await;
        let bug = make(
            &pool,
            project,
            user,
            "請求書の一括出力が失敗する",
            "請求書 一括出力",
            "closed",
            "bug",
        )
        .await;
        let other = make(
            &pool,
            other_project,
            user,
            "請求書の一括出力(別プロジェクト)",
            "請求書 一括出力",
            "closed",
            "task",
        )
        .await;

        let key = key_of(&pool, me).await;
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["請求書", "一括出力"]),
            2,
            8,
        )
        .await
        .expect("ok");
        let keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        assert!(
            keys.contains(&key_of(&pool, ok1).await),
            "完了・同一プロジェクト: {keys:?}"
        );
        assert!(
            keys.contains(&key_of(&pool, ok2).await),
            "resolved も完了分類: {keys:?}"
        );
        for (id, why) in [
            (me, "自分自身"),
            (open_one, "未完了"),
            (bug, "障害"),
            (other, "別プロジェクト"),
        ] {
            assert!(
                !keys.contains(&key_of(&pool, id).await),
                "{why} は候補に出さない: {keys:?}"
            );
        }
    }

    #[tokio::test]
    async fn candidates_need_at_least_min_hits_and_are_ordered_by_hits() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let me = make(&pool, project, user, "x", "", "open", "task").await;
        let one = make(&pool, project, user, "alphaの話", "", "closed", "task").await;
        let two = make(
            &pool,
            project,
            user,
            "alphaとbetaの話",
            "",
            "closed",
            "task",
        )
        .await;
        let three = make(
            &pool,
            project,
            user,
            "alpha beta gamma",
            "",
            "closed",
            "task",
        )
        .await;
        let key = key_of(&pool, me).await;

        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["alpha", "beta", "gamma"]),
            2,
            8,
        )
        .await
        .expect("ok");
        let keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        assert_eq!(
            keys,
            vec![key_of(&pool, three).await, key_of(&pool, two).await],
            "1語だけは出さない・一致が多い順"
        );
        assert!(!keys.contains(&key_of(&pool, one).await));

        // 件数の上限
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["alpha", "beta"]),
            1,
            1,
        )
        .await
        .expect("ok");
        assert_eq!(got.len(), 1);
        // 検索語が空なら、何も返さない(全件を返さない)
        assert!(
            find_pattern_candidates(&pool, &key, CandidateKind::Feature, &[], 1, 8)
                .await
                .expect("ok")
                .is_empty()
        );
    }

    #[test]
    fn overlapping_terms_count_once() {
        // 窓の重なり(端が 3 文字つながる)・包含は 1 件。離れた語は別々に数える
        assert_eq!(independent_hits(&["機能を追加す", "能を追加する"]), 1);
        assert_eq!(independent_hits(&["コメント欄に", "コメント欄"]), 1);
        assert_eq!(independent_hits(&["請求書", "一括出力"]), 2);
        assert_eq!(independent_hits(&["alpha", "beta", "gamma"]), 3);
        assert_eq!(independent_hits(&[]), 0);
        // 1 文字だけの共有は重なりとしない
        assert!(!terms_overlap("abc", "cde"));
        assert!(terms_overlap("abcd", "cdef"));
    }

    #[tokio::test]
    async fn overlapping_windows_of_one_phrase_are_not_two_hits() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let me = make(&pool, project, user, "x", "", "open", "task").await;
        let same_phrase = make(&pool, project, user, "機能を追加する", "", "closed", "task").await;
        let two_phrases = make(
            &pool,
            project,
            user,
            "請求書の話",
            "一括出力 も",
            "closed",
            "task",
        )
        .await;
        let key = key_of(&pool, me).await;
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["機能を追加す", "能を追加する", "請求書", "一括出力"]),
            2,
            8,
        )
        .await
        .expect("ok");
        let keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        assert_eq!(keys, vec![key_of(&pool, two_phrases).await], "{keys:?}");
        assert!(!keys.contains(&key_of(&pool, same_phrase).await));
    }

    #[tokio::test]
    async fn common_terms_are_not_counted_when_enough_peers_exist() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let me = make(&pool, project, user, "x", "", "open", "task").await;
        // ありふれた語(9件中 8件に出る)
        for i in 0..8 {
            make(
                &pool,
                project,
                user,
                &format!("機能を追加する その{i}"),
                "確認用の説明",
                "closed",
                "task",
            )
            .await;
        }
        // ありふれた語だけが一致するもの / 珍しい語が 2 つ一致するもの
        let generic_only = make(
            &pool,
            project,
            user,
            "機能を追加する 無関係",
            "確認用の説明",
            "closed",
            "task",
        )
        .await;
        let specific = make(
            &pool,
            project,
            user,
            "請求書を一括出力する",
            "請求書 一括出力 の実装",
            "closed",
            "task",
        )
        .await;
        let key = key_of(&pool, me).await;
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["機能を追加", "確認用", "請求書", "一括出力"]),
            2,
            20,
        )
        .await
        .expect("ok");
        let keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        assert_eq!(
            keys,
            vec![key_of(&pool, specific).await],
            "ありふれた語だけの一致は選ばない: {keys:?}"
        );
        assert!(!keys.contains(&key_of(&pool, generic_only).await));
    }

    #[tokio::test]
    async fn common_terms_still_count_in_small_projects() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let me = make(&pool, project, user, "x", "", "open", "task").await;
        // 比べる相手が 8 件未満なら、ありふれたかを判断できないので、従来どおり数える
        let a = make(
            &pool,
            project,
            user,
            "機能を追加する",
            "確認用",
            "closed",
            "task",
        )
        .await;
        make(
            &pool,
            project,
            user,
            "機能を追加する別件",
            "確認用",
            "closed",
            "task",
        )
        .await;
        let key = key_of(&pool, me).await;
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["機能を追加", "確認用"]),
            2,
            8,
        )
        .await
        .expect("ok");
        let keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        assert!(keys.contains(&key_of(&pool, a).await), "{keys:?}");
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn percent_and_underscore_in_terms_do_not_match_everything() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let me = make(&pool, project, user, "x", "", "open", "task").await;
        let _plain = make(&pool, project, user, "abcdef", "ghi", "closed", "task").await;
        let literal = make(
            &pool,
            project,
            user,
            "割引は100%適用",
            "a_b も確認",
            "closed",
            "task",
        )
        .await;
        let key = key_of(&pool, me).await;

        // `%` や `_` がワイルドカードとして働けば、"abcdef" にも当たってしまう
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["100%", "a_b"]),
            2,
            8,
        )
        .await
        .expect("ok");
        let keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        assert_eq!(keys, vec![key_of(&pool, literal).await]);
        let all = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["%", "_"]),
            1,
            8,
        )
        .await
        .expect("ok");
        assert_eq!(all.len(), 1, "% と _ は文字として検索する: {all:?}");
    }

    #[tokio::test]
    async fn followup_bugs_are_found_by_key_mention_in_same_project_only() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let other_project = create_test_project(&pool, "KBQ", user).await;
        let feature = make(&pool, project, user, "機能", "", "closed", "task").await;
        let fkey = key_of(&pool, feature).await;
        let by_desc = make(
            &pool,
            project,
            user,
            "不具合A",
            &format!("{fkey} のあとに発生"),
            "open",
            "bug",
        )
        .await;
        let by_title = make(
            &pool,
            project,
            user,
            &format!("{fkey} の不具合"),
            "",
            "open",
            "不具合",
        )
        .await;
        let not_bug = make(
            &pool,
            project,
            user,
            "通常",
            &format!("{fkey} を参照"),
            "open",
            "task",
        )
        .await;
        let other = make(
            &pool,
            other_project,
            user,
            "別PJの不具合",
            &format!("{fkey} の件"),
            "open",
            "bug",
        )
        .await;
        let by_comment = make(&pool, project, user, "不具合B", "", "open", "bug").await;
        sqlx::query("INSERT INTO tickets_comment (body, created_at, author_id, ticket_id) VALUES ($1, NOW(), $2, $3)")
            .bind(format!("原因は {fkey} の変更"))
            .bind(user as i64)
            .bind(by_comment as i64)
            .execute(&pool)
            .await
            .expect("comment");

        let got = find_followup_bugs(&pool, &fkey, 10).await.expect("ok");
        assert!(got.contains(&key_of(&pool, by_desc).await), "{got:?}");
        assert!(
            got.contains(&key_of(&pool, by_title).await),
            "旧来の日本語種別も障害: {got:?}"
        );
        assert!(
            got.contains(&key_of(&pool, by_comment).await),
            "コメントの言及も拾う: {got:?}"
        );
        assert!(
            !got.contains(&key_of(&pool, not_bug).await),
            "障害でないものは出さない"
        );
        assert!(
            !got.contains(&key_of(&pool, other).await),
            "別プロジェクトは出さない"
        );
        assert_eq!(
            find_followup_bugs(&pool, &fkey, 1).await.expect("ok").len(),
            1,
            "上限"
        );
    }

    #[tokio::test]
    async fn incident_kind_selects_only_completed_incidents_of_the_same_project() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let other = create_test_project(&pool, "KBQ", user).await;
        let me = make(&pool, project, user, "x", "", "open", "bug").await;
        let t = "請求書 一括出力";
        let done_bug = make(
            &pool,
            project,
            user,
            "請求書の出力が失敗",
            t,
            "closed",
            "bug",
        )
        .await;
        let labeled = make(
            &pool,
            project,
            user,
            "請求書の出力が遅い",
            t,
            "resolved",
            "task",
        )
        .await;
        let label_id: i64 = sqlx::query_scalar("INSERT INTO m_label (name, color, created_at, project_id) VALUES ('type:infra', '#000000', NOW(), $1) RETURNING id::int8")
            .bind(project)
            .fetch_one(&pool)
            .await
            .expect("label");
        sqlx::query(
            "INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)",
        )
        .bind(labeled)
        .bind(label_id)
        .execute(&pool)
        .await
        .expect("link");
        // 除外されるべきもの
        let feature = make(
            &pool,
            project,
            user,
            "請求書の一括出力",
            t,
            "closed",
            "task",
        )
        .await;
        let open_bug = make(
            &pool,
            project,
            user,
            "請求書の出力で未完了",
            t,
            "in_progress",
            "bug",
        )
        .await;
        let other_bug = make(&pool, other, user, "請求書の出力(別PJ)", t, "closed", "bug").await;

        let key = key_of(&pool, me).await;
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Incident,
            &terms(&["請求書", "一括出力"]),
            2,
            8,
        )
        .await
        .expect("ok");
        let mut keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        keys.sort();
        let mut want = vec![key_of(&pool, done_bug).await, key_of(&pool, labeled).await];
        want.sort();
        assert_eq!(keys, want, "完了済みの障害(種別・ラベル)だけ: {keys:?}");
        for (id, why) in [
            (feature, "機能追加は出さない"),
            (open_bug, "未完了は出さない"),
            (other_bug, "別プロジェクトは出さない"),
            (me, "自分自身は出さない"),
        ] {
            assert!(!keys.contains(&key_of(&pool, id).await), "{why}: {keys:?}");
        }
        // Feature の側には、障害は出ない(逆向きの確認)
        let got = find_pattern_candidates(
            &pool,
            &key,
            CandidateKind::Feature,
            &terms(&["請求書", "一括出力"]),
            2,
            8,
        )
        .await
        .expect("ok");
        let keys: Vec<String> = got.iter().map(|c| c.key.clone()).collect();
        assert_eq!(keys, vec![key_of(&pool, feature).await], "{keys:?}");
    }

    #[tokio::test]
    async fn incident_detection_uses_type_or_label() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let user = create_test_user(&pool, "kbpat").await;
        let project = create_test_project(&pool, "KBP", user).await;
        let normal = make(&pool, project, user, "通常", "", "open", "task").await;
        let bug = make(&pool, project, user, "障害", "", "open", "bug").await;
        let labeled = make(&pool, project, user, "ラベルで障害", "", "open", "task").await;
        let label_id: i64 = sqlx::query_scalar("INSERT INTO m_label (name, color, created_at, project_id) VALUES ('type:incident', '#000000', NOW(), $1) RETURNING id::int8")
            .bind(project)
            .fetch_one(&pool)
            .await
            .expect("label");
        sqlx::query(
            "INSERT INTO tickets_ticket_labels (ticketmodel_id, labelmodel_id) VALUES ($1, $2)",
        )
        .bind(labeled)
        .bind(label_id)
        .execute(&pool)
        .await
        .expect("link");
        assert_eq!(
            is_not_incident(&pool, &key_of(&pool, normal).await)
                .await
                .expect("ok"),
            Some(true)
        );
        assert_eq!(
            is_not_incident(&pool, &key_of(&pool, bug).await)
                .await
                .expect("ok"),
            Some(false)
        );
        assert_eq!(
            is_not_incident(&pool, &key_of(&pool, labeled).await)
                .await
                .expect("ok"),
            Some(false)
        );
        assert_eq!(is_not_incident(&pool, "NOPE-1").await.expect("ok"), None);
    }
}
