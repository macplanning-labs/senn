//! 通知の宛先の確認(アクセス制御の再設計 C-5。詳細設計書 §10.1)
//!
//! 通知(アプリ内・メール)は、送る時点で、宛先がそのチケットを見られる場合だけ送る。
//! 今の動作は「確認せずに送る」なので、試運転のスイッチ(チケット)に従う。
//! - `off`    : 送る(今の動作)
//! - `shadow` : 送る。新しい規則なら送らない宛先を記録する
//! - `on`     : 新しい規則で見える宛先だけに送る

use sqlx::PgPool;

use crate::domain::access::{can, Action, Decision, Principal};
use crate::infrastructure::access::{
    facts_repo,
    shadow::{self, Direction, Mode, Resource},
    viewer_repo,
};

/// この宛先に、このチケットの通知を送ってよいか
pub async fn may_notify(
    pool: &PgPool,
    ticket_id: i32,
    user_id: i32,
    route: &'static str,
) -> anyhow::Result<bool> {
    let mode = shadow::mode(Resource::Ticket);
    if mode == Mode::Off {
        return Ok(true);
    }
    let visible = match facts_repo::facts_for_ticket(pool, ticket_id).await? {
        None => false,
        Some(facts) => {
            match viewer_repo::load(pool, Principal::Human { user_id }, viewer_repo::today_utc())
                .await?
            {
                // 無効化されたユーザーには送らない
                None => false,
                Some(v) => can(&v, Action::Read, &facts) == Decision::Allow,
            }
        }
    };
    Ok(match mode {
        Mode::On => visible,
        _ => {
            if !visible {
                shadow::record(
                    pool,
                    Resource::Ticket,
                    Some(user_id),
                    route,
                    vec![(Direction::NewlyHidden, ticket_id as i64)],
                );
            }
            true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        create_test_project, create_test_ticket, create_test_user, test_pool,
    };

    #[tokio::test]
    async fn shadow_keeps_sending_but_records_hidden_recipients() {
        let Some(pool) = test_pool().await else {
            return;
        };
        let author = create_test_user(&pool, "rcp-a").await;
        let outsider = create_test_user(&pool, "rcp-o").await;
        let project = create_test_project(&pool, "RCP", author).await;
        let ticket = create_test_ticket(&pool, project, "RCP", author).await;
        sqlx::query("UPDATE m_team SET visibility = 'private' WHERE id = (SELECT tt.team_id FROM tickets_ticket tt WHERE tt.id = $1::int8)")
            .bind(ticket as i64)
            .execute(&pool)
            .await
            .unwrap();
        // 既定(試運転): Private のチケットを見られない人にも、今は送る。ただし記録する
        assert!(may_notify(&pool, ticket, outsider, "test").await.unwrap());
        let mut n = 0i64;
        for _ in 0..50 {
            n = sqlx::query_scalar("SELECT count(*) FROM access_shadow_diff WHERE resource = 'ticket' AND resource_id = $1 AND user_id = $2")
                .bind(ticket as i64)
                .bind(outsider as i64)
                .fetch_one(&pool)
                .await
                .unwrap();
            if n > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(n, 1, "送らない宛先として記録される");
    }
}
