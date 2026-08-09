/// domain/services/ai_service.rs — AI推論サービス
///
/// Django apps/ai/services.py + apps/ai/prompts/*.py の移植。
/// Ollama(ローカル推論)をプライマリ、OpenAI APIをフォールバックとして使用する
/// 三層防御: 1. Ollama → 2. OpenAI → 3. 安全なデフォルト値。
/// AIが落ちてもアプリは止まらない設計をそのまま踏襲する。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AppConfig;

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

// =============================================================================
// Ollama / OpenAI 呼び出し
// =============================================================================

async fn call_ollama(config: &AppConfig, prompt: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let result = client
        .post(format!("{}/api/generate", config.ollama_url))
        .timeout(std::time::Duration::from_secs(config.ollama_timeout_secs))
        .json(&serde_json::json!({
            "model": config.ollama_model,
            "prompt": prompt,
            "stream": false,
            "format": "json",
        }))
        .send()
        .await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Value>().await {
                Ok(data) => data.get("response").and_then(|v| v.as_str()).map(|s| s.to_string()),
                Err(e) => {
                    tracing::warn!("Ollama response parse failed: {:?}", e);
                    None
                }
            }
        }
        Ok(resp) => {
            tracing::warn!("Ollama call failed with status: {}", resp.status());
            None
        }
        Err(e) => {
            tracing::warn!("Ollama call failed: {:?}", e);
            None
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
- Task Status: in_progress (Possible values: backlog, todo, in_progress, done, canceled, resolved)

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
