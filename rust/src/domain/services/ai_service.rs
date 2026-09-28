/// domain/services/ai_service.rs — AI推論サービス
///
/// Django apps/ai/services.py + apps/ai/prompts/*.py の移植。
/// Ollama(ローカル推論)をプライマリ、OpenAI APIをフォールバックとして使用する
/// 三層防御: 1. Ollama → 2. OpenAI → 3. 安全なデフォルト値。
/// AIが落ちてもアプリは止まらない設計をそのまま踏襲する。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AppConfig;

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

fn in_flight_set() -> &'static Mutex<HashSet<i32>> {
    static SET: OnceLock<Mutex<HashSet<i32>>> = OnceLock::new();
    SET.get_or_init(|| Mutex::new(HashSet::new()))
}


// =============================================================================
// レスポンス型 + デフォルト値(AI障害時のフォールバック)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryPointResult {
    pub suggested_points: i32,
    pub confidence_score: f64,
    pub reason: String,
}

pub fn default_story_point() -> StoryPointResult {
    StoryPointResult {
        suggested_points: 2,
        confidence_score: 0.0,
        reason: "AI service unavailable. Default baseline (2 points) applied.".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintHealthAlert {
    pub task_id: Value,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintHealthResult {
    pub risk_level: String,
    pub summary: String,
    pub alerts: Vec<Value>,
    pub suggested_actions: Vec<Value>,
}

pub fn default_sprint_health() -> SprintHealthResult {
    SprintHealthResult {
        risk_level: "medium".to_string(),
        summary: "AI analysis unavailable. Please review manually.".to_string(),
        alerts: vec![],
        suggested_actions: vec![Value::String("Review task progress manually.".to_string())],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextAnalysisResult {
    pub context_loaded: bool,
    pub rule_violations: Vec<Value>,
    pub implementation_hint: String,
}

pub fn default_context_analysis() -> ContextAnalysisResult {
    ContextAnalysisResult {
        context_loaded: false,
        rule_violations: vec![],
        implementation_hint: "AI service unavailable. Please review manually.".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseAnalysisResult {
    pub has_future_challenges: bool,
    pub wiki_draft: Option<Value>,
    pub suggested_backlog_tickets: Vec<Value>,
}

pub fn default_close_analysis() -> CloseAnalysisResult {
    CloseAnalysisResult {
        has_future_challenges: false,
        wiki_draft: None,
        suggested_backlog_tickets: vec![],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratePromptTextResult {
    pub prompt_text: String,
    /// "hybrid" | "template_fallback"
    #[serde(rename = "generationMode")]
    pub generation_mode: String,
}

/// Ollama 呼び出し失敗時の詳細（API エラーレスポンス用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaCallError {
    pub error: String,
    pub error_code: String,
    pub model: String,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

fn format_elapsed_secs(elapsed_ms: u64) -> String {
    format!("{:.1}", elapsed_ms as f64 / 1000.0)
}

fn ollama_error(
    code: &'static str,
    message: String,
    model: &str,
    elapsed_ms: u64,
    timeout_secs: Option<u64>,
) -> OllamaCallError {
    OllamaCallError {
        error: message,
        error_code: code.to_string(),
        model: model.to_string(),
        elapsed_ms,
        timeout_secs,
    }
}

// =============================================================================
// Ollama / OpenAI 呼び出し
// =============================================================================

async fn call_ollama(config: &AppConfig, prompt: &str) -> Option<String> {
    match call_ollama_generate(config, prompt, true).await {
        Ok(text) => Some(text),
        Err(e) => {
            tracing::warn!("Ollama call failed: {} ({})", e.error, e.error_code);
            None
        }
    }
}

async fn call_ollama_text(config: &AppConfig, prompt: &str) -> Result<String, OllamaCallError> {
    call_ollama_generate(config, prompt, false).await
}

async fn call_ollama_generate(
    config: &AppConfig,
    prompt: &str,
    json_format: bool,
) -> Result<String, OllamaCallError> {
    let started = std::time::Instant::now();
    let model = config.ollama_model.clone();
    let timeout_secs = config.ollama_timeout_secs;
    let elapsed_ms = || started.elapsed().as_millis() as u64;

    let mut body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
    });
    if json_format {
        body["format"] = serde_json::Value::String("json".to_string());
    }

    let client = reqwest::Client::new();
    let result = client
        .post(format!("{}/api/generate", config.ollama_url))
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .json(&body)
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Value>().await {
                Ok(data) => match data.get("response").and_then(|v| v.as_str()) {
                    Some(text) if !text.trim().is_empty() => Ok(text.to_string()),
                    _ => Err(ollama_error(
                        "ollama_empty_response",
                        format!(
                            "Ollama の応答が空です（モデル: {}、{}秒）",
                            model,
                            format_elapsed_secs(elapsed_ms())
                        ),
                        &model,
                        elapsed_ms(),
                        Some(timeout_secs),
                    )),
                },
                Err(e) => {
                    tracing::warn!("Ollama response parse failed: {:?}", e);
                    Err(ollama_error(
                        "ollama_parse_failed",
                        format!(
                            "Ollama の応答を解釈できません（モデル: {}、{}秒）",
                            model,
                            format_elapsed_secs(elapsed_ms())
                        ),
                        &model,
                        elapsed_ms(),
                        Some(timeout_secs),
                    ))
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            let lowered = body_text.to_lowercase();
            let code = if status.as_u16() == 404 || lowered.contains("not found") {
                "ollama_model_not_found"
            } else {
                "ollama_http_error"
            };
            let message = if code == "ollama_model_not_found" {
                format!(
                    "モデル「{}」が見つかりません（{}秒）。`ollama pull {}` を実行してください。",
                    model,
                    format_elapsed_secs(elapsed_ms()),
                    model
                )
            } else {
                format!(
                    "Ollama エラー HTTP {}（モデル: {}、{}秒）",
                    status,
                    model,
                    format_elapsed_secs(elapsed_ms())
                )
            };
            tracing::warn!("Ollama call failed with status: {} body: {}", status, &body_text[..body_text.len().min(200)]);
            Err(ollama_error(code, message, &model, elapsed_ms(), Some(timeout_secs)))
        }
        Err(e) => {
            if e.is_timeout() {
                Err(ollama_error(
                    "ollama_timeout",
                    format!(
                        "Ollama がタイムアウトしました（モデル: {}、{}秒経過、上限 {}秒）",
                        model,
                        format_elapsed_secs(elapsed_ms()),
                        timeout_secs
                    ),
                    &model,
                    elapsed_ms(),
                    Some(timeout_secs),
                ))
            } else if e.is_connect() || e.is_request() {
                Err(ollama_error(
                    "ollama_connection_failed",
                    format!(
                        "Ollama に接続できません（モデル: {}、{}秒）。起動状態と OLLAMA_URL を確認してください。",
                        model,
                        format_elapsed_secs(elapsed_ms())
                    ),
                    &model,
                    elapsed_ms(),
                    Some(timeout_secs),
                ))
            } else {
                tracing::warn!("Ollama call failed: {:?}", e);
                Err(ollama_error(
                    "ollama_unavailable",
                    format!(
                        "Ollama が利用できません（モデル: {}、{}秒）",
                        model,
                        format_elapsed_secs(elapsed_ms())
                    ),
                    &model,
                    elapsed_ms(),
                    Some(timeout_secs),
                ))
            }
        }
    }
}

async fn call_openai(config: &AppConfig, prompt: &str) -> Option<String> {
    let api_key = config.openai_api_key.as_ref()?;

    let client = reqwest::Client::new();
    let result = client
        .post("https://api.openai.com/v1/chat/completions")
        .timeout(std::time::Duration::from_secs(config.openai_timeout_secs))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": config.openai_model,
            "messages": [
                {"role": "system", "content": "You are a project management AI assistant. Always respond with valid JSON."},
                {"role": "user", "content": prompt},
            ],
            "response_format": {"type": "json_object"},
            "temperature": 0.3,
        }))
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Value>().await {
                Ok(data) => data
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("message"))
                    .and_then(|m| m.get("content"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                Err(e) => {
                    tracing::warn!("OpenAI response parse failed: {:?}", e);
                    None
                }
            }
        }
        Ok(resp) => {
            tracing::warn!("OpenAI call failed with status: {}", resp.status());
            None
        }
        Err(e) => {
            tracing::warn!("OpenAI call failed: {:?}", e);
            None
        }
    }
}

/// AIレスポンス文字列(```json ... ``` ラッパーの可能性あり)をJSONとしてパースする。
fn parse_json(raw: Option<String>) -> Option<Value> {
    let raw = raw?;
    let cleaned: String = raw
        .trim()
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");

    match serde_json::from_str(&cleaned) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("Failed to parse AI JSON response: {:?} (raw: {})", e, &raw[..raw.len().min(200)]);
            None
        }
    }
}

/// AI応答から ``` ラッパーを除去する。
fn clean_text_response(raw: &str) -> String {
    raw.trim()
        .lines()
        .filter(|line| !line.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Ollama → OpenAI の順で呼び出し、最初に得られた応答文字列を返す。
async fn call_with_fallback(config: &AppConfig, prompt: &str) -> Option<String> {
    if let Some(r) = call_ollama(config, prompt).await {
        return Some(r);
    }
    call_openai(config, prompt).await
}

// =============================================================================
// 公開API(各分析関数)
// =============================================================================

pub async fn suggest_story_points(
    config: &AppConfig,
    title: &str,
    description: &str,
    team_rules: &str,
    language: &str,
) -> StoryPointResult {
    let prompt = format!(
        r#"
# Purpose
Analyze the user's task title and description to estimate the implementation complexity and uncertainty using Fibonacci story points.

# Estimation Criteria
- 1: Trivial task (e.g., typo fix, single-line text change). Takes almost no effort.
- 2: Standard baseline task (The "Normal" minimum task for a developer). Requires writing a few lines of code, tests, and getting a code review.
- 3: Moderate task (Multiple steps or files affected, but path is clear).
- 5: Complex/Uncertain task (Involves external APIs, minor refactoring, or slight unknowns).
- 8 or more: High uncertainty or massive scope. (Highly recommend splitting the task).

# Input Context
- User Language: {language}
- Task Title: {title}
- Task Description: {description}
- Associated Team Rules: {team_rules}

# Output Format (JSON)
{{
  "suggested_points": 1 | 2 | 3 | 5 | 8 | 13 | 21,
  "confidence_score": 0.0 to 1.0,
  "reason": "Explain WHY you chose this point based on the Estimation Criteria. Write this explanation in [ {language} ]."
}}
"#,
        language = language,
        title = title,
        description = if description.is_empty() { "(no description)" } else { description },
        team_rules = if team_rules.is_empty() { "(no team rules)" } else { team_rules },
    );

    let parsed = parse_json(call_with_fallback(config, &prompt).await);

    if let Some(p) = parsed.as_ref().filter(|p| p.get("suggested_points").is_some()) {
        StoryPointResult {
            suggested_points: p.get("suggested_points").and_then(|v| v.as_i64()).unwrap_or(2) as i32,
            confidence_score: p.get("confidence_score").and_then(|v| v.as_f64()).unwrap_or(0.5),
            reason: p.get("reason").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    } else {
        default_story_point()
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn analyze_sprint_health(
    config: &AppConfig,
    project_prefix: &str,
    target_start_date: &str,
    target_end_date: &str,
    tasks_json: &Value,
    language: &str,
) -> SprintHealthResult {
    let tasks_json_data = serde_json::to_string_pretty(tasks_json).unwrap_or_else(|_| "[]".to_string());

    let prompt = format!(
        r#"
# Purpose
You are the core project management AI of an agile development tool. Analyze the provided project metadata and task list to detect delivery risks and provide optimization advice.

# Input Data (System Identifiers)
The following data uses strict system identifiers (DB values). Never change these strings during your analysis.
- Project Status: active (Possible values: active, completed, paused, canceled)
- Task Status: in_progress (Possible values: backlog, open, in_progress, resolved, closed, canceled)

[Project Details]
- Prefix: {project_prefix}
- Target Schedule: {target_start_date} to {target_end_date}

[Task List to Analyze]
{tasks_json_data}

# System Rules for Analysis
1. Evaluate team velocity based on "story_points" (Null means unestimated).
2. Remember that "story_points=2" is the standard baseline task, and "1" is a trivial task. If there are tasks with 8 or more points, flag them as "High Risk (Need Split)".
3. Analyze dependency graphs using task IDs.

# Output Language
You must generate the final response in [ {language} ]. (e.g., ja, en)

# Output Format (JSON)
Return your analysis strictly in the following JSON structure. All text fields in the response MUST be written in the specified Output Language.

{{
  "risk_level": "high" | "medium" | "low",
  "summary": "Brief summary of the project health in the requested language.",
  "alerts": [
    {{
      "task_id": 123,
      "reason": "Reason for the alert (e.g., task is blocked, points are too high) in the requested language."
    }}
  ],
  "suggested_actions": [
    "Action item 1 in the requested language.",
    "Action item 2 in the requested language."
  ]
}}
"#,
        project_prefix = project_prefix,
        target_start_date = target_start_date,
        target_end_date = target_end_date,
        tasks_json_data = tasks_json_data,
        language = language,
    );

    let parsed = parse_json(call_with_fallback(config, &prompt).await);

    if let Some(p) = parsed.as_ref().filter(|p| p.get("risk_level").is_some()) {
        SprintHealthResult {
            risk_level: p.get("risk_level").and_then(|v| v.as_str()).unwrap_or("medium").to_string(),
            summary: p.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            alerts: p.get("alerts").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
            suggested_actions: p.get("suggested_actions").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
        }
    } else {
        default_sprint_health()
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn analyze_ticket_context(
    config: &AppConfig,
    ticket_id: i32,
    ticket_title: &str,
    ticket_description: &str,
    ticket_status: &str,
    story_points: Option<i32>,
    project_prefix: &str,
    associated_rules_text: &str,
    language: &str,
) -> ContextAnalysisResult {
    let user_language = if language == "ja" { "Japanese" } else { "English" };

    let prompt = format!(
        r#"
# Purpose
You are the development context coordinator. Analyze the given Ticket along with its associated Wiki/Team Rules (Context) to provide implementation advice or detect inconsistencies.

# Input Data (System Identifiers)
[Target Ticket]
- ID: {ticket_id}
- Prefix: {project_prefix}
- Current Status: {ticket_status}
- Story Points: {story_points}
- Title: {ticket_title}
- Description: {ticket_description}

[Associated Wiki & Team Rules (Strict Context)]
Below are the rules and technical specifications pinned to this ticket. You MUST respect these constraints during your analysis.
{associated_rules_text}

# Output Language
You must generate the final response in [ {user_language} ].

# Output Format (JSON)
Return your analysis strictly in the following JSON structure:
{{
  "context_loaded": true,
  "rule_violations": [
    {{
      "rule_title": "Title of the violated rule",
      "warning": "Explanation of how the current ticket description violates this rule in the requested language."
    }}
  ],
  "implementation_hint": "Tailored implementation advice combining the ticket goal and the team rules in the requested language."
}}
"#,
        ticket_id = ticket_id,
        project_prefix = project_prefix,
        ticket_status = ticket_status,
        story_points = story_points.map(|p| p.to_string()).unwrap_or_else(|| "Not set".to_string()),
        ticket_title = ticket_title,
        ticket_description = if ticket_description.is_empty() { "(no description)" } else { ticket_description },
        associated_rules_text = if associated_rules_text.is_empty() { "(no rules linked)" } else { associated_rules_text },
        user_language = user_language,
    );

    let parsed = parse_json(call_with_fallback(config, &prompt).await);

    if let Some(p) = parsed.as_ref().filter(|p| p.get("context_loaded").is_some()) {
        ContextAnalysisResult {
            context_loaded: p.get("context_loaded").and_then(|v| v.as_bool()).unwrap_or(false),
            rule_violations: p.get("rule_violations").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
            implementation_hint: p.get("implementation_hint").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }
    } else {
        default_context_analysis()
    }
}

pub async fn analyze_ticket_close(
    config: &AppConfig,
    ticket_title: &str,
    ticket_description: &str,
    comments_text: &str,
    language: &str,
) -> CloseAnalysisResult {
    let user_language = if language == "ja" { "Japanese" } else { "English" };

    let prompt = format!(
        r#"
# Purpose
The developer is about to close this ticket. Analyze the task description and the discussion log to extract "Future Challenges", "Technical Debts", or "Next Action Ideas" that should not be forgotten.

# Input Context
- Task Title: {ticket_title}
- Task Description: {ticket_description}
- Discussion Log / Comments:
{comments_text}

# Instructions
1. Find any tasks or refactoring ideas that the team decided to postpone or skip for this specific ticket.
2. Formulate them into a structured Markdown format suitable for the team's Wiki (TeamRule).
3. Draft a title for a potential "Backlog" ticket if it deserves to be a separate task.

# Output Language
You must generate the final response in [ {user_language} ].

# Output Format (JSON)
Return strictly in JSON format:
{{
  "has_future_challenges": true | false,
  "wiki_draft": {{
    "category": "Future Challenges" or "Technical Debt",
    "title": "A concise title for the Wiki page",
    "content_markdown": "Detailed markdown listing what to do next, why it was postponed, and potential approach."
  }},
  "suggested_backlog_tickets": [
    {{
      "title": "Concise ticket title for the Backlog",
      "description": "Brief description of the task."
    }}
  ]
}}
"#,
        ticket_title = ticket_title,
        ticket_description = if ticket_description.is_empty() { "(no description)" } else { ticket_description },
        comments_text = if comments_text.is_empty() { "(no comments)" } else { comments_text },
        user_language = user_language,
    );

    let parsed = parse_json(call_with_fallback(config, &prompt).await);

    if let Some(p) = parsed.as_ref().filter(|p| p.get("has_future_challenges").is_some()) {
        CloseAnalysisResult {
            has_future_challenges: p.get("has_future_challenges").and_then(|v| v.as_bool()).unwrap_or(false),
            wiki_draft: p.get("wiki_draft").cloned().filter(|v| !v.is_null()),
            suggested_backlog_tickets: p.get("suggested_backlog_tickets").and_then(|v| v.as_array()).cloned().unwrap_or_default(),
        }
    } else {
        default_close_analysis()
    }
}

#[derive(Debug, Serialize)]
pub struct AiStatus {
    pub ollama: OllamaStatus,
    pub openai: OpenAiStatus,
}

#[derive(Debug, Serialize)]
pub struct OllamaStatus {
    pub url: String,
    pub connected: bool,
    pub model: String,
    #[serde(rename = "available_models")]
    pub available_models: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct OpenAiStatus {
    pub configured: bool,
}

const DEFAULT_AI_PROMPT_TEMPLATE: &str = r#"Readmeを参照してください。
bulk-read-guardを利用してください
GHAは利用不可のため、ローカルCI 手動デプロイを利用してください
実装はターミナルを起動してHaikuで実装するので、実装計画と詳細設計書、タスクリスト、Haikuへ渡すプロンプトを作成してください。"#;

fn resolve_ai_prompt_template(db_value: Option<&str>) -> String {
    db_value
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_AI_PROMPT_TEMPLATE.to_string())
}

enum PromptContentMode {
    Hybrid {
        description_summary: String,
        comments_summary: String,
        situation_summary: String,
    },
    FullTextFallback {
        description: String,
        comments_text: String,
        situation_summary: String,
    },
}

const FULL_TEXT_FALLBACK_STEP0: &str = r#"### 0. 作業前の必須ステップ
SENN 側の Ollama 要約が利用できないため、以下の説明・コメント全文を読み、
作業開始前にあなた自身で要点を整理してからタスクを実行してください。

"#;

fn extract_markdown_section(text: &str, heading: &str) -> Option<String> {
    let needle = format!("## {heading}");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start_matches('\n');
    let end = rest.find("\n## ").unwrap_or(rest.len());
    let section = rest[..end].trim();
    if section.is_empty() {
        None
    } else {
        Some(section.to_string())
    }
}

fn build_cached_ai_prompt_template(
    ticket_key: &str,
    title: &str,
    status: &str,
    content: PromptContentMode,
    links_text: &str,
    custom_rules: &str,
) -> String {
    let mission = r#"### {mission_num}. ミッション
上記の説明・コメント・状況サマリを読んだうえで作業してください。
状況サマリの「推奨アクション」を最優先の指針にしてください。

- **bug_fix**: 指摘された不具合の調査・修正・検証に集中する。説明に既存の設計書パスがある場合はそれを読み、**同内容の設計書一式を新規作成し直さない**（必要なら差分の短い修正計画とタスクリストのみ）。
- **continue_impl**: 未完部分の実装を進める。必要なら実装計画・詳細設計・タスクリストを作成/更新する。
- **verify_only**: 完了定義（DoD）に沿って検証し、結果を報告する。
- **close_ready**: クローズ可能か客観的に判断し、理由を報告する。

共通:
- スコープ外は別チケット化を提案する。
- `/docs` 配下の関連設計書があれば先に読む。
"#;

    let common_constraints = r#"### {constraints_num}. 共通制約
- 新規DBクエリは Repository 層経由
- `/docs` 配下の正本を読んでから作業
- スコープ外は別チケット化を提案
"#;

    match content {
        PromptContentMode::Hybrid {
            description_summary,
            comments_summary,
            situation_summary,
        } => {
            format!(
                r#"あなたはシニアソフトウェアエンジニア兼テックリードです。
以下のチケット情報と現状のコンテキストを確認し、指示に従ってタスクを実行してください。

### 1. 対象チケット情報
チケットキー: {ticket_key}
タイトル: {title}
ステータス: {status}

#### 説明（要約）
{description_summary}

### 2. コメント履歴（要約）
{comments_summary}

### 3. 参照リンク
{links_text}

### 4. 状況サマリ（参考・ローカルAIによる短文分析）
{situation_summary}

{mission}

### 6. 制約・ルール（プロジェクト設定）
{custom_rules}

{common_constraints}

それでは、まずは状況の確認から開始してください。
"#,
                ticket_key = ticket_key,
                title = title,
                status = status,
                description_summary = description_summary,
                comments_summary = comments_summary,
                links_text = links_text,
                situation_summary = situation_summary,
                mission = mission.replace("{mission_num}", "5"),
                custom_rules = custom_rules,
                common_constraints = common_constraints.replace("{constraints_num}", "7"),
            )
        }
        PromptContentMode::FullTextFallback {
            description,
            comments_text,
            situation_summary,
        } => {
            let comments_body = if comments_text.trim().is_empty() {
                "（コメントなし）".to_string()
            } else {
                comments_text
            };
            format!(
                r#"あなたはシニアソフトウェアエンジニア兼テックリードです。

{step0}### 1. 対象チケット情報
チケットキー: {ticket_key}
タイトル: {title}
ステータス: {status}

#### 説明（全文）
{description}

### 2. コメント履歴（全文）
{comments_body}

### 3. 参照リンク
{links_text}

### 4. 状況サマリ（参考・ローカルAIによる短文分析）
{situation_summary}

{mission}

### 6. 制約・ルール（プロジェクト設定）
{custom_rules}

{common_constraints}

それでは、まずは状況の確認から開始してください。
"#,
                step0 = FULL_TEXT_FALLBACK_STEP0,
                ticket_key = ticket_key,
                title = title,
                status = status,
                description = description,
                comments_body = comments_body,
                links_text = links_text,
                situation_summary = situation_summary,
                mission = mission.replace("{mission_num}", "5"),
                custom_rules = custom_rules,
                common_constraints = common_constraints.replace("{constraints_num}", "7"),
            )
        }
    }
}

/// ハイブリッド方式: テンプレート組立関数（同期 API 用 — 全文 + 状況サマリ）
fn build_dev_ai_prompt_template(
    ticket_key: &str,
    title: &str,
    status: &str,
    description: &str,
    comments_text: &str,
    situation_summary: &str,
    ai_prompt_template: &str,
) -> String {
    format!(
        r#"あなたはシニアソフトウェアエンジニア兼テックリードです。
以下のチケット情報と現状のコンテキストを確認し、指示に従ってタスクを実行してください。

### 1. 対象チケット情報
チケットキー: {ticket_key}
タイトル: {title}
ステータス: {status}
内容・コンテキスト:
{description}

### 2. コメント履歴（重要な論点を把握すること）
{comments_text}

### 3. SENN側の状況サマリ（参考・ローカルAIによる短文分析）
{situation_summary}

### 4. ミッション
上記の説明・コメント・状況サマリを読んだうえで作業してください。
状況サマリの「推奨アクション」を最優先の指針にしてください。

- **bug_fix**: 指摘された不具合の調査・修正・検証に集中する。説明に既存の設計書パスがある場合はそれを読み、**同内容の設計書一式を新規作成し直さない**（必要なら差分の短い修正計画とタスクリストのみ）。
- **continue_impl**: 未完部分の実装を進める。必要なら実装計画・詳細設計・タスクリストを作成/更新する。
- **verify_only**: 完了定義（DoD）に沿って検証し、結果を報告する。
- **close_ready**: クローズ可能か客観的に判断し、理由を報告する。

共通:
- スコープ外は別チケット化を提案する。
- `/docs` 配下の関連設計書があれば先に読む。

### 5. 制約・ルール（ボーイスカウト精神）
- 修正や新規実装を行う場合、開発ルール（CLAUDE.md / AGENTS.md 等）に準拠すること。
- 新規DBクエリは必ずRepository層を経由させること。
- スコープ外の項目は本チケットに含めず、別チケット化を提案すること。
- **必ず `/docs` 配下の正本（要件定義書・概要設計書・アプリ処理方式設計書・ナラティブ）を読んでから作業を開始すること。**
- 開発標準を遵守すること。
- プロジェクト設定のおまじない: {ai_prompt_template}

それでは、まずは状況の確認から開始してください。
"#,
        ticket_key = ticket_key,
        title = title,
        status = status,
        description = description,
        comments_text = comments_text,
        situation_summary = situation_summary,
        ai_prompt_template = ai_prompt_template,
    )
}

/// Ollama で短い状況サマリのみを生成（メタプロンプトは設計書 §3）
async fn generate_situation_summary(
    config: &AppConfig,
    ticket_key: &str,
    title: &str,
    status: &str,
    description: &str,
    comments_text: &str,
    team_rules_text: &str,
) -> Result<String, OllamaCallError> {
    let prompt = format!(
        r#"あなたはチケット分析アシスタントです。
以下のチケット情報を読み、開発AI向けプロンプトに差し込む【状況サマリ】だけを出力してください。
前置き・挨拶・コードフェンス・「はい」等は禁止。指定フォーマットのMarkdownのみ。

# チケット
キー: {ticket_key}
タイトル: {title}
ステータス: {status}
説明:
{description}

# コメント
{comments_text}

# チームルール（参考）
{team_rules_text}

# 推奨アクション判定ルール（上から優先。該当したらその種別を選ぶ）
1. **bug_fix** — 実行時の不具合（500/401/403/NaN/panic/接続失敗/タイムアウト/「Unable To Extract Key!」等）が**主な残件**で、実装・ビルドは一通りある（または大部分完了）。「一部未実装」と書いてあっても、実行時エラーが主因なら bug_fix を選ぶ。
2. **continue_impl** — 未着手、新規API・画面・PoC、未配線の追加実装、設計判断待ちで実装がこれから。
3. **verify_only** — 実装は完了済みで、残りは手動/実機/DoD 検証・確認のみ。
4. **close_ready** — 残作業なし、または本文が「完了」「実施済み」「マージ済み」「デプロイ済み」等でクローズ可能。ステータスが open/in_progress でも本文が完了なら close_ready とし、ステータス整合で矛盾を指摘する。

# 判定の注意
- 046型: 大規模実装済み + 実行時500 → bug_fix（continue_impl にしない）
- 043型: 未配線・追加適用が主 → continue_impl
- 088型: 本文完了 + open → close_ready + ステータス不整合

# 必ずこの見出し付き箇条書きで出力（各1〜2行）
- フェーズ: （実装中 / 検証待ち / 完了相当 / 調査中 など）
- 残作業: （具体的な残件。なければ「なし（クローズ判定可）」）
- 推奨アクション: （次のいずれか1つを選び、短い理由を付ける）
  - bug_fix … 既存実装の不具合修正が主
  - continue_impl … 未完の実装を続ける
  - verify_only … 実装済みの検証・DoD確認が主
  - close_ready … 残作業なしでクローズ判断が主
- 既存ドキュメント: （説明・コメントに出てきた設計書・タスクリスト等のパス。なければ「記載なし」。再作成不要なら「既存を更新/参照」と明記）
- コメント論点: （ブロッカー・合意・未決。なければ「特になし」）
- ステータス整合: （ステータスと本文の矛盾があれば指摘。なければ「整合」）

# 制約
- 全体で日本語 500文字以内
- 実装計画・詳細設計・タスクリストの本文は書かない
- プロンプト全文や「あなたはシニア〜」は書かない
- 事実にない情報を捏造しない（コメントが空なら論点は「特になし」）
- 上記判定ルールに従い、実行時不具合が主残件なら continue_impl ではなく bug_fix を選ぶ
- バグ修正が主なら推奨アクションは bug_fix とし、新規の設計書一式作成を推奨しない
"#,
        ticket_key = ticket_key,
        title = title,
        status = status,
        description = description,
        comments_text = comments_text,
        team_rules_text = team_rules_text,
    );

    call_ollama_text(config, &prompt).await
}

/// BG キャッシュ用: 説明・コメント要約 + 状況サマリを 1 回の Ollama 呼び出しで生成
async fn generate_content_and_situation_summary(
    config: &AppConfig,
    ticket_key: &str,
    title: &str,
    status: &str,
    description: &str,
    comments_text: &str,
    team_rules_text: &str,
) -> Result<String, OllamaCallError> {
    let comments_block = if comments_text.trim().is_empty() {
        "（コメントなし）".to_string()
    } else {
        comments_text.to_string()
    };

    let prompt = format!(
        r#"あなたはチケット分析アシスタントです。
以下のチケット情報を読み、開発AI向けプロンプトに差し込む要約だけを出力してください。
前置き・挨拶・コードフェンス・「はい」等は禁止。指定フォーマットのMarkdownのみ。

# チケット
キー: {ticket_key}
タイトル: {title}
ステータス: {status}
説明:
{description}

# コメント
{comments_block}

# チームルール（参考）
{team_rules_text}

# 推奨アクション判定ルール（上から優先。該当したらその種別を選ぶ）
1. **bug_fix** — 実行時の不具合が主な残件
2. **continue_impl** — 未着手・追加実装・設計判断待ち
3. **verify_only** — 実装完了、検証のみ
4. **close_ready** — 残作業なし、クローズ可能

# 必ず次の3見出しだけをこの順で出力

## 説明要約
（200〜400日本語文字以内。空なら「（説明なし）」）

## コメント要約
（300〜600日本語文字以内。空コメントなら「（コメントなし）」。論点・合意・未決を優先）

## 状況サマリ
- フェーズ: ...
- 残作業: ...
- 推奨アクション: （bug_fix / continue_impl / verify_only / close_ready のいずれか1つ）
- 既存ドキュメント: ...
- コメント論点: ...
- ステータス整合: ...

# 制約
- 事実にない情報を捏造しない
- プロンプト全文や「あなたはシニア〜」は書かない
"#,
        ticket_key = ticket_key,
        title = title,
        status = status,
        description = description,
        comments_block = comments_block,
        team_rules_text = team_rules_text,
    );

    call_ollama_text(config, &prompt).await
}

/// フォールバック文言（Ollama失敗時）
const SITUATION_SUMMARY_FALLBACK: &str = r#"- フェーズ: （自動判定不可）
- 残作業: 説明とコメント全文を直接読み判断してください
- 推奨アクション: continue_impl （ローカルAIサマリ取得失敗のため既定）
- 既存ドキュメント: 説明文中のパスを確認
- コメント論点: 特になし（自動判定不可）
- ステータス整合: 説明とステータスを照合してください"#;

pub async fn generate_dev_ai_prompt(
    config: &AppConfig,
    ticket_key: &str,
    _project_prefix: &str,
    title: &str,
    status: &str,
    description: &str,
    comments_text: &str,
    team_rules_text: &str,
    _language: &str,
) -> Result<GeneratePromptTextResult, OllamaCallError> {
    let (summary, generation_mode) = match generate_situation_summary(
        config,
        ticket_key,
        title,
        status,
        description,
        comments_text,
        team_rules_text,
    )
    .await
    {
        Ok(s) => {
            let cleaned = clean_text_response(&s);
            if cleaned.is_empty() {
                (SITUATION_SUMMARY_FALLBACK.to_string(), "template_fallback")
            } else {
                (cleaned, "hybrid")
            }
        }
        Err(_) => (SITUATION_SUMMARY_FALLBACK.to_string(), "template_fallback"),
    };

    let prompt_text = build_dev_ai_prompt_template(
        ticket_key,
        title,
        status,
        description,
        comments_text,
        &summary,
        DEFAULT_AI_PROMPT_TEMPLATE,
    );

    Ok(GeneratePromptTextResult {
        prompt_text,
        generation_mode: generation_mode.to_string(),
    })
}

/// Ollamaの疎通チェック(GET /api/tags、3秒タイムアウト)。
pub async fn check_ai_status(config: &AppConfig) -> AiStatus {
    let client = reqwest::Client::new();
    let result = client
        .get(format!("{}/api/tags", config.ollama_url))
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await;

    let (connected, models) = match result {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Value>().await {
                Ok(data) => {
                    let models = data
                        .get("models")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default();
                    (true, models)
                }
                Err(_) => (false, vec![]),
            }
        }
        _ => (false, vec![]),
    };

    AiStatus {
        ollama: OllamaStatus {
            url: config.ollama_url.clone(),
            connected,
            model: config.ollama_model.clone(),
            available_models: models,
        },
        openai: OpenAiStatus {
            configured: config.openai_api_key.is_some(),
        },
    }
}

use crate::infrastructure::repositories::ai_repo;

/// AIプロンプトをキャッシュに保存（コメント追加後のバックグラウンド処理）
pub async fn generate_and_cache_ai_prompt(
    ticket_id: i32,
    pool: &sqlx::PgPool,
    config: &AppConfig,
) -> anyhow::Result<()> {
    {
        let mut in_flight = in_flight_set()
            .lock()
            .map_err(|e| anyhow::anyhow!("in_flight lock poisoned: {e}"))?;
        if !in_flight.insert(ticket_id) {
            tracing::debug!(ticket_id, "generate_and_cache_ai_prompt skipped (in flight)");
            return Ok(());
        }
    }

    let result = generate_and_cache_ai_prompt_inner(ticket_id, pool, config).await;

    if let Ok(mut in_flight) = in_flight_set().lock() {
        in_flight.remove(&ticket_id);
    }

    result
}

async fn generate_and_cache_ai_prompt_inner(
    ticket_id: i32,
    pool: &sqlx::PgPool,
    config: &AppConfig,
) -> anyhow::Result<()> {
    let ticket = ai_repo::find_ticket_for_ai(pool, ticket_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("ticket not found: {}", ticket_id))?;

    let description = ticket.description.clone();
    let comments_text = ai_repo::build_comments_text(pool, ticket_id).await?;
    let team_rules_text =
        ai_repo::build_associated_rules_text(pool, ticket_id, ticket.team_id).await?;
    let links_text = ai_repo::build_links_text(pool, ticket_id).await?;
    let ai_prompt_template = resolve_ai_prompt_template(
        ai_repo::get_project_ai_prompt_template(pool, ticket.project_id)
            .await?
            .as_deref(),
    );

    let (content, generation_mode) = match generate_content_and_situation_summary(
        config,
        &ticket.ticket_key,
        &ticket.title,
        &ticket.status,
        &description,
        &comments_text,
        &team_rules_text,
    )
    .await
    {
        Ok(raw) => {
            let cleaned = clean_text_response(&raw);
            let description_summary = extract_markdown_section(&cleaned, "説明要約")
                .unwrap_or_else(|| {
                    if description.trim().is_empty() {
                        "（説明なし）".to_string()
                    } else {
                        description.chars().take(400).collect::<String>()
                    }
                });
            let comments_summary = extract_markdown_section(&cleaned, "コメント要約")
                .unwrap_or_else(|| {
                    if comments_text.trim().is_empty() {
                        "（コメントなし）".to_string()
                    } else {
                        comments_text.chars().take(600).collect::<String>()
                    }
                });
            let situation_summary = extract_markdown_section(&cleaned, "状況サマリ")
                .filter(|s| !s.is_empty());

            if let Some(situation_summary) = situation_summary {
                (
                    PromptContentMode::Hybrid {
                        description_summary,
                        comments_summary,
                        situation_summary,
                    },
                    "hybrid".to_string(),
                )
            } else {
                (
                    PromptContentMode::FullTextFallback {
                        description: description.clone(),
                        comments_text: comments_text.clone(),
                        situation_summary: SITUATION_SUMMARY_FALLBACK.to_string(),
                    },
                    "full_text_fallback".to_string(),
                )
            }
        }
        Err(_) => (
            PromptContentMode::FullTextFallback {
                description: description.clone(),
                comments_text: comments_text.clone(),
                situation_summary: SITUATION_SUMMARY_FALLBACK.to_string(),
            },
            "full_text_fallback".to_string(),
        ),
    };

    let prompt_text = build_cached_ai_prompt_template(
        &ticket.ticket_key,
        &ticket.title,
        &ticket.status,
        content,
        &links_text,
        &ai_prompt_template,
    );

    ai_repo::save_ticket_ai_prompt(pool, ticket_id, &prompt_text, &generation_mode).await?;

    Ok(())
}

#[cfg(test)]
mod ai_prompt_cache_tests {
    use super::*;

    #[test]
    fn resolve_ai_prompt_template_uses_default_for_none_and_empty() {
        assert_eq!(
            resolve_ai_prompt_template(None),
            DEFAULT_AI_PROMPT_TEMPLATE.to_string()
        );
        assert_eq!(resolve_ai_prompt_template(Some("")), DEFAULT_AI_PROMPT_TEMPLATE);
        assert_eq!(resolve_ai_prompt_template(Some("  ")), DEFAULT_AI_PROMPT_TEMPLATE);
    }

    #[test]
    fn resolve_ai_prompt_template_trims_custom() {
        assert_eq!(
            resolve_ai_prompt_template(Some("  custom rules  ")),
            "custom rules"
        );
    }

    #[test]
    fn extract_markdown_section_parses_headings() {
        let text = "## 説明要約\n要約本文\n\n## コメント要約\nコメント本文";
        assert_eq!(
            extract_markdown_section(text, "説明要約").as_deref(),
            Some("要約本文")
        );
        assert_eq!(
            extract_markdown_section(text, "コメント要約").as_deref(),
            Some("コメント本文")
        );
        assert!(extract_markdown_section(text, "存在しない").is_none());
    }
}
