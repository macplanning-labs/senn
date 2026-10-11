/// presentation/handlers/ai_agent_bulk_api.rs — AI 用: チケットを依存関係ごとまとめて登録する(DEMO-000221)
///
/// 実装計画からタスクを起票すると、1 件ごとに作成・更新・依存の登録が要り、30 件で約 90 回の呼び出しになる。
/// 途中で失敗すると半端な状態が残る。ここでは、計画の中の仮の名前(ref)で親・依存を書いてもらい、
/// 1 回の呼び出し・1 つのトランザクションで登録する(全部か、何も登録しないか)。
/// `dry_run` は、同じ処理を最後に取り消す(番号は仮)。
use std::collections::{HashMap, HashSet};

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::json;

use crate::domain::access::Action;
use crate::domain::models::ticket_api::TicketWriteIn;
use crate::infrastructure::repositories::{ai_run_repo, ticket_repo};
use crate::presentation::handlers::ai_agent_api::{
    ai_db_error, ai_ticket_gate, bad_request, check_ai_cycle, check_ai_status,
    check_ai_story_points, default_priority, default_ticket_type, insert_ai_ticket,
    resolve_ai_assignees, resolve_ai_create_target, resolve_ai_labels, AiCaller,
};
use crate::presentation::state::AppState;

/// 1 回で登録できる件数の上限
const MAX_TICKETS: usize = 100;
const ROUTE: &str = "POST /api/v1/ai-agent/tickets/bulk/";

#[derive(Deserialize)]
pub struct BulkAiTicketsIn {
    pub project_prefix: Option<String>,
    pub team_id: Option<i32>,
    pub team_slug: Option<String>,
    /// true なら、確かめるだけで何も登録しない
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub tickets: Vec<BulkAiTicketIn>,
}

#[derive(Deserialize)]
pub struct BulkAiTicketIn {
    /// この一括の中での仮の名前(例: T1)。親・依存の指定に使う
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default = "default_ticket_type")]
    pub ticket_type: String,
    pub status: Option<String>,
    pub story_points: Option<i16>,
    pub start_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub cycle: Option<i32>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignee_usernames: Vec<String>,
    /// 親(この一括の ref か、既存のチケットのキー)
    pub parent: Option<String>,
    /// 先に終わらせるもの(この一括の ref か、既存のチケットのキー)。
    /// 「X が終わらないと、このチケットに進めない」を blocks(X → このチケット)で登録する
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// 参照先: この一括の何番目か、既存のチケットの ID か
#[derive(Clone, Copy)]
enum RefTarget {
    New(usize),
    Existing(i32),
}

fn at(i: usize, t: &BulkAiTicketIn) -> String {
    format!("tickets[{}](ref: {})", i, t.ref_name)
}

/// POST /api/v1/ai-agent/tickets/bulk/
pub async fn bulk_create(
    State(state): State<AppState>,
    caller: AiCaller,
    Json(body): Json<BulkAiTicketsIn>,
) -> impl IntoResponse {
    let tickets = &body.tickets;
    if tickets.is_empty() {
        return bad_request("tickets を 1 件以上指定してください");
    }
    if tickets.len() > MAX_TICKETS {
        return bad_request(format!(
            "1 回で登録できるのは {} 件までです(指定: {} 件)",
            MAX_TICKETS,
            tickets.len()
        ));
    }

    // 1. 形の確認(ref の重複・空のタイトル)
    let mut index_of: HashMap<&str, usize> = HashMap::new();
    for (i, t) in tickets.iter().enumerate() {
        let r = t.ref_name.trim();
        if r.is_empty() {
            return bad_request(format!("tickets[{}]: ref は必須です", i));
        }
        if t.title.trim().is_empty() {
            return bad_request(format!("{}: title は必須です", at(i, t)));
        }
        if index_of.insert(r, i).is_some() {
            return bad_request(format!("{}: ref が重複しています", at(i, t)));
        }
    }

    // 2. 作成先と、作成できるか
    let tgt = match resolve_ai_create_target(
        &state,
        &caller,
        body.project_prefix.as_deref(),
        body.team_id,
        body.team_slug.as_deref(),
        ROUTE,
    )
    .await
    {
        Ok(t) => t,
        Err(resp) => return resp,
    };

    // 3. 参照(親・依存)を解決する。ref に無ければ既存のチケットのキーとして探す
    let mut existing: HashMap<String, i32> = HashMap::new();
    // 既存のキーを先にまとめて引く(見えること・依存を足せることも確かめる)
    let mut wanted: Vec<(String, Action)> = Vec::new();
    for t in tickets {
        if let Some(p) = &t.parent {
            if !index_of.contains_key(p.trim()) {
                wanted.push((p.trim().to_string(), Action::Read));
            }
        }
        for d in &t.depends_on {
            if !index_of.contains_key(d.trim()) {
                // 既存のチケットに「この後に続くもの」を足すので、書き込みの権限を求める(単体の依存の追加と同じ)
                wanted.push((d.trim().to_string(), Action::Write));
            }
        }
    }
    for (key, action) in &wanted {
        if !existing.contains_key(key) {
            match ticket_repo::resolve_ticket_id(&state.pool, key).await {
                Ok(Some(id)) => {
                    existing.insert(key.clone(), id);
                }
                Ok(None) => {
                    return bad_request(format!(
                        "'{}' が見つかりません(この一括の ref にも、既存のチケットのキーにもありません)",
                        key
                    ))
                }
                Err(e) => return ai_db_error(e),
            }
        }
        if let Err(resp) = ai_ticket_gate(&state, &caller, key, *action, ROUTE).await {
            return resp;
        }
    }
    let resolve = |s: &str| -> Option<RefTarget> {
        index_of
            .get(s.trim())
            .map(|&i| RefTarget::New(i))
            .or_else(|| existing.get(s.trim()).map(|&id| RefTarget::Existing(id)))
    };

    let mut parents: Vec<Option<RefTarget>> = Vec::with_capacity(tickets.len());
    // edges[i] = i より先に終わらせるもの
    let mut deps: Vec<Vec<RefTarget>> = Vec::with_capacity(tickets.len());
    for (i, t) in tickets.iter().enumerate() {
        let parent = match &t.parent {
            Some(p) => match resolve(p) {
                Some(RefTarget::New(j)) if j == i => {
                    return bad_request(format!("{}: 自分自身を親にはできません", at(i, t)))
                }
                Some(target) => Some(target),
                None => return bad_request(format!("{}: 親 '{}' が見つかりません", at(i, t), p)),
            },
            None => None,
        };
        parents.push(parent);
        let mut list: Vec<RefTarget> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for d in &t.depends_on {
            if !seen.insert(d.trim().to_string()) {
                continue; // 同じものを 2 回書いても 1 本にする
            }
            match resolve(d) {
                Some(RefTarget::New(j)) if j == i => {
                    return bad_request(format!("{}: 自分自身には依存できません", at(i, t)))
                }
                Some(target) => list.push(target),
                None => {
                    return bad_request(format!("{}: 依存先 '{}' が見つかりません", at(i, t), d))
                }
            }
        }
        deps.push(list);
    }

    // 4. この一括の中での循環(依存・親)を、登録の前に見つける
    if let Some(cycle) = find_dependency_cycle(&deps) {
        let names: Vec<&str> = cycle
            .iter()
            .map(|&i| tickets[i].ref_name.as_str())
            .collect();
        return bad_request(format!(
            "依存関係が循環しています: {}(何も登録していません)",
            names.join(" → ")
        ));
    }
    for start in 0..tickets.len() {
        let mut cur = start;
        let mut steps = 0;
        while let Some(RefTarget::New(p)) = parents[cur] {
            steps += 1;
            if p == start || steps > tickets.len() {
                return bad_request(format!(
                    "{}: 親子関係が循環しています(何も登録していません)",
                    at(start, &tickets[start])
                ));
            }
            cur = p;
        }
    }

    // 5. 各項目の確認(状態・ストーリーポイント・Cycle)
    for (i, t) in tickets.iter().enumerate() {
        if let Some(status) = &t.status {
            match check_ai_status(&state.pool, tgt.project_id, Some(tgt.team_id), status).await {
                Ok(None) => {}
                Ok(Some(msg)) => return bad_request(format!("{}: {}", at(i, t), msg)),
                Err(e) => return ai_db_error(e),
            }
        }
        if let Some(msg) = check_ai_story_points(t.story_points) {
            return bad_request(format!("{}: {}", at(i, t), msg));
        }
        if let Some(cycle_id) = t.cycle {
            match check_ai_cycle(&state.pool, tgt.project_id, tgt.team_id, cycle_id).await {
                Ok(None) => {}
                Ok(Some(msg)) => return bad_request(format!("{}: {}", at(i, t), msg)),
                Err(e) => return ai_db_error(e),
            }
        }
    }

    // 6. 担当者(試しに実行のときは、Guest の追加などの副作用を残さない)
    let mut assignees: Vec<Vec<i32>> = Vec::with_capacity(tickets.len());
    for t in tickets {
        match resolve_ai_assignees(&state, &tgt, &t.assignee_usernames, !body.dry_run).await {
            Ok(ids) => assignees.push(ids),
            Err(resp) => return resp,
        }
    }

    // 7. 登録(1 つのトランザクション)
    let author_id = caller.comment_author();
    let ai_user_id = caller.legacy.user_id;
    let mut tx = match state.pool.begin().await {
        Ok(t) => t,
        Err(e) => return ai_db_error(e.into()),
    };
    macro_rules! abort {
        ($resp:expr) => {{
            if let Err(e) = tx.rollback().await {
                tracing::error!("transaction rollback failed: {:?}", e);
            }
            return $resp;
        }};
    }

    let mut ids: Vec<i32> = Vec::with_capacity(tickets.len());
    for (i, t) in tickets.iter().enumerate() {
        let label_ids =
            match resolve_ai_labels(&mut tx, tgt.project_id, tgt.team_id, &t.labels).await {
                Ok(v) => v,
                Err(e) => abort!(ai_db_error(e)),
            };
        let parent = match parents[i] {
            Some(RefTarget::Existing(id)) => Some(id),
            _ => None, // この一括の親は、全部作ってから付ける
        };
        let input = TicketWriteIn {
            title: t.title.trim().to_string(),
            description: t.description.clone(),
            status: t.status.clone().unwrap_or_else(|| "open".to_string()),
            priority: t.priority.clone(),
            ticket_type: t.ticket_type.clone(),
            assignees: assignees[i].clone(),
            reviewers: Vec::new(),
            category: None,
            project: tgt.project_id,
            milestone: None,
            parent,
            start_date: t.start_date,
            due_date: t.due_date,
            labels: label_ids,
            story_points: t.story_points,
            cycle: t.cycle,
            team_id: Some(tgt.team_id),
            linked_rules: Vec::new(),
        };
        match insert_ai_ticket(&mut tx, &input, author_id, caller.legacy.key_id).await {
            Ok(id) => ids.push(id),
            Err(e) => {
                tracing::error!("[AI 一括登録] {} の作成に失敗: {:?}", at(i, t), e);
                abort!(ai_db_error(e))
            }
        }
    }

    for (i, parent) in parents.iter().enumerate() {
        if let Some(RefTarget::New(p)) = parent {
            if let Err(e) = sqlx::query("UPDATE tickets_ticket SET parent_id = $1 WHERE id = $2")
                .bind(ids[*p])
                .bind(ids[i])
                .execute(&mut *tx)
                .await
            {
                abort!(ai_db_error(e.into()))
            }
        }
    }

    let id_of = |r: RefTarget| match r {
        RefTarget::New(j) => ids[j],
        RefTarget::Existing(id) => id,
    };
    let mut edges: Vec<(i32, i32)> = Vec::new();
    for (i, list) in deps.iter().enumerate() {
        for &d in list {
            let (from, to) = (id_of(d), ids[i]);
            use ticket_repo::CreateDependencyResult as R;
            match ticket_repo::create_dependency_conn(&mut tx, from, to, "blocks", ai_user_id).await
            {
                Ok(R::Success(_)) | Ok(R::Duplicate) => edges.push((from, to)),
                Ok(R::CircularDependency) => abort!(bad_request(format!(
                    "{}: 既存のチケットの依存関係とつなぐと循環します(何も登録していません)",
                    at(i, &tickets[i])
                ))),
                Ok(R::SelfReference) | Ok(R::ToTaskNotFound) => abort!(bad_request(format!(
                    "{}: 依存関係を登録できません(何も登録していません)",
                    at(i, &tickets[i])
                ))),
                Err(e) => abort!(ai_db_error(e)),
            }
        }
    }

    // 番号を引く(試しに実行のときは仮の番号)
    let mut all_ids: Vec<i32> = ids.clone();
    all_ids.extend(edges.iter().flat_map(|&(f, t)| [f, t]));
    let keys: HashMap<i32, String> = match sqlx::query_as::<_, (i32, String)>(
        "SELECT id::int4, ticket_key FROM tickets_ticket WHERE id = ANY($1)",
    )
    .bind(&all_ids)
    .fetch_all(&mut *tx)
    .await
    {
        Ok(rows) => rows.into_iter().collect(),
        Err(e) => abort!(ai_db_error(e.into())),
    };

    if body.dry_run {
        if let Err(e) = tx.rollback().await {
            tracing::error!("transaction rollback failed: {:?}", e);
        }
    } else {
        if let Err(e) = tx.commit().await {
            return ai_db_error(e.into());
        }
        // 「着手」の状態で作ったものは、開始日が空なら入れる(単体の作成と同じ)
        for (i, t) in tickets.iter().enumerate() {
            if t.status.is_some() && t.start_date.is_none() {
                if let Err(e) =
                    ai_run_repo::set_start_date_if_started(&state.pool, ids[i], ai_user_id).await
                {
                    tracing::warn!(
                        "[AI 一括登録] 処理=開始日の設定 結果=失敗 影響=開始日が空のまま | ticket_id={} {:?}",
                        ids[i],
                        e
                    );
                }
            }
        }
    }

    let key = |id: i32| keys.get(&id).cloned().unwrap_or_default();
    let created: Vec<serde_json::Value> = tickets
        .iter()
        .zip(&ids)
        .map(|(t, &id)| {
            json!({
                "ref": t.ref_name,
                "id": if body.dry_run { serde_json::Value::Null } else { json!(id) },
                "ticket_key": key(id),
                "title": t.title.trim(),
            })
        })
        .collect();
    let dependencies: Vec<serde_json::Value> = edges
        .iter()
        .map(|&(f, t)| json!({"from": key(f), "to": key(t), "dependency_type": "blocks"}))
        .collect();
    let status = if body.dry_run {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    (
        status,
        Json(json!({
            "dry_run": body.dry_run,
            "note": if body.dry_run {
                "試しに実行しました。何も登録していません(ticket_key は仮の番号です)"
            } else {
                "登録しました"
            },
            "tickets": created,
            "dependencies": dependencies,
        })),
    )
        .into_response()
}

/// この一括の中の依存(deps[i] = i より先に終わらせるもの)に循環があれば、その並び(最初と最後が同じ)を返す
fn find_dependency_cycle(deps: &[Vec<RefTarget>]) -> Option<Vec<usize>> {
    // 0 = 未訪問, 1 = たどっている途中, 2 = 済み
    fn visit(
        i: usize,
        deps: &[Vec<RefTarget>],
        state: &mut [u8],
        stack: &mut Vec<usize>,
    ) -> Option<Vec<usize>> {
        state[i] = 1;
        stack.push(i);
        for d in &deps[i] {
            if let RefTarget::New(j) = *d {
                if state[j] == 1 {
                    let pos = stack.iter().position(|&x| x == j).unwrap_or(0);
                    let mut cycle: Vec<usize> = stack[pos..].to_vec();
                    cycle.push(j);
                    // stack は「後のもの → 先のもの」の向きなので、「先 → 後」に直す
                    cycle.reverse();
                    return Some(cycle);
                }
                if state[j] == 0 {
                    if let Some(c) = visit(j, deps, state, stack) {
                        return Some(c);
                    }
                }
            }
        }
        stack.pop();
        state[i] = 2;
        None
    }
    let mut state = vec![0u8; deps.len()];
    for i in 0..deps.len() {
        if state[i] == 0 {
            let mut stack = Vec::new();
            if let Some(c) = visit(i, deps, &mut state, &mut stack) {
                return Some(c);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(list: &[usize]) -> Vec<RefTarget> {
        list.iter().map(|&i| RefTarget::New(i)).collect()
    }

    #[test]
    fn no_cycle_in_a_chain_or_with_existing_tickets() {
        let deps = vec![vec![], n(&[0]), {
            let mut v = n(&[1]);
            v.push(RefTarget::Existing(999));
            v
        }];
        assert!(find_dependency_cycle(&deps).is_none());
    }

    #[test]
    fn finds_a_three_step_cycle() {
        // 0 は 2 の後、1 は 0 の後、2 は 1 の後
        let deps = vec![n(&[2]), n(&[0]), n(&[1])];
        let c = find_dependency_cycle(&deps).expect("循環を見つける");
        assert_eq!(c.first(), c.last());
        assert_eq!(c.len(), 4);
    }
}
