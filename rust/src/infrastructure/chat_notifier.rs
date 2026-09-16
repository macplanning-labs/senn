/// infrastructure/chat_notifier.rs — チャットツールへのWebhook送信
///
/// Slack/Google Chat/Teamsは同一のJSON形式({"text": ...})、
/// ChatworkはAPIトークン+room_idでform-urlencoded POSTする。

use serde_json::json;

use crate::domain::models::chat_integration_api::ChatIntegrationOut;

/// 1件のチャット連携先にメッセージを送信する。呼び出し側は失敗してもチケット操作を止めない。
pub async fn send(integration: &ChatIntegrationOut, text: &str) -> anyhow::Result<()> {
    match integration.provider.as_str() {
        "slack" | "google_chat" | "teams" => send_webhook_json(integration, text).await,
        "chatwork" => send_chatwork(integration, text).await,
        other => anyhow::bail!("unknown provider: {}", other),
    }
}

async fn send_webhook_json(integration: &ChatIntegrationOut, text: &str) -> anyhow::Result<()> {
    let url = integration
        .webhook_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("webhook_url not set"))?;

    let client = reqwest::Client::new();
    let res = client
        .post(url)
        .json(&json!({ "text": text }))
        .send()
        .await?;

    if !res.status().is_success() {
        anyhow::bail!("chat webhook returned status {}", res.status());
    }
    Ok(())
}

async fn send_chatwork(integration: &ChatIntegrationOut, text: &str) -> anyhow::Result<()> {
    let token = integration
        .api_token
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("api_token not set"))?;
    let room_id = integration
        .room_id
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("room_id not set"))?;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("https://api.chatwork.com/v2/rooms/{}/messages", room_id))
        .header("X-ChatWorkToken", token)
        .form(&[("body", text)])
        .send()
        .await?;

    if !res.status().is_success() {
        anyhow::bail!("chatwork API returned status {}", res.status());
    }
    Ok(())
}
