/// domain/services/ai_kick_policy.rs — 「AIに対応させる」の安全装置の判定(純粋関数。DB を使わない)
///
/// 設計: docs/design/実装計画_AI自動ワークフローとInsights.md §3.6.1(暴走の防止)。
///
/// 原則: **止める判断は AI の外側で、機械的に行う**。AI に「止まれ」と頼む方式は使わない。
/// - 起動の可否(`check`): 緊急停止・許可・回数・キューの長さ・予算を、順に確認する。
/// - 実行中の継続の可否(`decide_heartbeat`): ハートビート(ランナーが 30 秒ごとに送る使用量)ごとに、
///   上限・予算・増え方(サーキットブレーカー)・緊急停止・取り消しを確認し、止めるなら理由を返す。
///   ランナーはこの応答の `stop` を受けたら即座にプロセスを終了する。
/// - 予算は**未設定では有効にできない**(`Limits` が無ければ起動を拒否する。「無制限」は存在しない)。
/// - **緊急度(優先度の区分)で扱いを変える**(2026-10-07)。お客様に迷惑がかかる緊急のバグ(セキュリティ・データ漏洩など)は、
///   トークンの予算では止めない。機能追加は通常。他の作業をブロックしているもの(ボトルネック)は優先度を上げる。
///   **ただし、暴走の防止(1 実行の上限・使い方の急増・緊急停止)は、緊急でも変わらない。**
/// - 予算(日・週)は**警告で知らせ、止めるのは上限(既定は予算の 2 倍)だけ**。業務を止めないため。
use serde::{Deserialize, Serialize};
use std::fmt;

/// 上限の設定。管理者が明示的に保存する(既定値は画面の初期値の案で、保存されるまで有効にならない)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    /// 1 実行の最大ターン数
    pub max_turns: u32,
    /// 1 実行の最大の実時間(分)
    pub max_minutes: u32,
    /// 1 実行の最大の加重トークン
    pub max_weighted_tokens: u64,
    /// システム全体の 1 日(JST)の予算(加重トークン)
    pub daily_budget_system: u64,
    /// システム全体の直近 7 日間の予算(加重トークン)。AI 自動実行が、人の作業の利用枠を使い切らないための上限
    pub weekly_budget_system: u64,
    /// プロジェクトごとの 1 日(JST)の予算(加重トークン)
    pub daily_budget_project: u64,
    /// 1 人あたりの起動回数(1 時間・1 日)
    pub per_user_hour: u32,
    pub per_user_day: u32,
    /// 1 チケットあたりの起動回数(24 時間)
    pub per_ticket_24h: u32,
    /// 取得待ちの最大件数
    pub max_queue: u32,
    /// 1 分あたりの加重トークンがこれを超えたら暴走とみなす(サーキットブレーカー)
    pub burn_rate_per_min: u64,
    /// 直近の起動がこの回数続けて失敗したら、緊急停止を作動させる
    pub max_consecutive_failures: u32,
    /// 予算の何 % に達したら、通常の優先度の起動を拒否するか(それまでは警告・確認)。既定 200
    pub hard_ceiling_percent: u32,
    /// 緊急の起動の、1 実行の上限(ターン・時間・トークン)の倍率(%)。既定 200
    pub emergency_cap_percent: u32,
    /// 予算を超える起動の確認を、管理者だけに許すか
    pub override_admin_only: bool,
}

impl Default for Limits {
    /// 画面の初期値の案。**2026-10-07 に、実際の使用量(Claude Code のログ)に合わせて見直した値。**
    /// 目的は「暴走を止める」ことで、普通の作業を止めないこと。実測(9 月以降の中規模な作業 107 件):
    /// 1 件あたり 中央値 1.2M・40 分・94 ターン、上位 1% でも 13M・173 分・290 ターン。
    /// 使い方の速さは、5 分平均で上位 1% が約 330k/分、1 分間の最大が約 1.4M。
    /// 週の上限の目安は約 570M(使用量の画面の「今週 64%」= 367M からの逆算。目安であって正確ではない)。
    fn default() -> Self {
        Limits {
            // 上位 1% の約 1.4 倍(普通の作業は止めず、無限ループは数時間で止める)
            max_turns: 400,
            max_minutes: 180,
            // 実測の最大(16M)の約 2 倍。週の上限の約 5%
            max_weighted_tokens: 30_000_000,
            // 実測の 1 日の使用量(30M〜150M)に対し、AI 自動実行が占める上限
            daily_budget_system: 60_000_000,
            daily_budget_project: 40_000_000,
            // 週の上限の目安(約 570M)の約 25%。人の作業を妨げないための上限
            weekly_budget_system: 150_000_000,
            per_user_hour: 10,
            per_user_day: 30,
            per_ticket_24h: 3,
            max_queue: 5,
            // 実測の 1 分間の最大(約 1.4M)より上。暴走(並列の無限ループなど)だけを捉える
            burn_rate_per_min: 2_000_000,
            max_consecutive_failures: 3,
            hard_ceiling_percent: 200,
            emergency_cap_percent: 200,
            override_admin_only: false,
        }
    }
}

impl Limits {
    /// 保存してよい値か。0 や極端に大きい値(実質の無制限)を拒否する。
    pub fn validate(&self) -> Result<(), String> {
        let positive = [
            ("maxTurns", u64::from(self.max_turns)),
            ("maxMinutes", u64::from(self.max_minutes)),
            ("maxWeightedTokens", self.max_weighted_tokens),
            ("dailyBudgetSystem", self.daily_budget_system),
            ("dailyBudgetProject", self.daily_budget_project),
            ("weeklyBudgetSystem", self.weekly_budget_system),
            ("perUserHour", u64::from(self.per_user_hour)),
            ("perUserDay", u64::from(self.per_user_day)),
            ("perTicket24h", u64::from(self.per_ticket_24h)),
            ("maxQueue", u64::from(self.max_queue)),
            ("burnRatePerMin", self.burn_rate_per_min),
            (
                "maxConsecutiveFailures",
                u64::from(self.max_consecutive_failures),
            ),
        ];
        for (name, v) in positive {
            if v == 0 {
                return Err(format!(
                    "{name} は 1 以上を指定してください(無制限にはできません)"
                ));
            }
        }
        if self.max_turns > 500 {
            return Err("maxTurns は 500 以下にしてください".into());
        }
        if self.max_minutes > 240 {
            return Err("maxMinutes は 240 以下にしてください".into());
        }
        if self.max_weighted_tokens > 50_000_000 {
            return Err("maxWeightedTokens は 50,000,000 以下にしてください".into());
        }
        if self.daily_budget_system > 1_000_000_000 {
            return Err("dailyBudgetSystem は 1,000,000,000 以下にしてください".into());
        }
        if self.weekly_budget_system > 10_000_000_000 {
            return Err("weeklyBudgetSystem は 10,000,000,000 以下にしてください".into());
        }
        if self.daily_budget_system > self.weekly_budget_system {
            return Err("1 日の予算は、1 週間の予算以下にしてください".into());
        }
        if self.daily_budget_project > self.daily_budget_system {
            return Err("プロジェクトの予算は、システム全体の予算以下にしてください".into());
        }
        if self.max_weighted_tokens > self.daily_budget_project {
            return Err("1 実行の上限は、プロジェクトの日次予算以下にしてください".into());
        }
        if self.per_user_hour > self.per_user_day {
            return Err("1 時間あたりの回数は、1 日あたりの回数以下にしてください".into());
        }
        if self.max_queue > 50 {
            return Err("maxQueue は 50 以下にしてください".into());
        }
        if !(100..=1000).contains(&self.hard_ceiling_percent) {
            return Err("hardCeilingPercent は 100〜1000 で指定してください".into());
        }
        if !(100..=500).contains(&self.emergency_cap_percent) {
            return Err("emergencyCapPercent は 100〜500 で指定してください".into());
        }
        // 緊急でも、暴走の防止の上限は有限であること(1 実行 100M 以下)
        if self.max_weighted_tokens * u64::from(self.emergency_cap_percent) / 100 > 100_000_000 {
            return Err("緊急の 1 実行の上限は、100,000,000 以下にしてください".into());
        }
        Ok(())
    }

    /// ランナーに渡す 1 実行の上限(通常)。
    pub fn caps(&self) -> Caps {
        Caps {
            max_turns: self.max_turns,
            max_minutes: self.max_minutes,
            max_weighted_tokens: self.max_weighted_tokens,
        }
    }

    /// 優先度の区分に応じた 1 実行の上限。緊急は倍率をかける(ただし有限。ターン 1000・480 分まで)。
    pub fn caps_for(&self, class: PriorityClass) -> Caps {
        if class != PriorityClass::Emergency {
            return self.caps();
        }
        let m = u64::from(self.emergency_cap_percent);
        Caps {
            max_turns: ((u64::from(self.max_turns) * m / 100).min(1000)) as u32,
            max_minutes: ((u64::from(self.max_minutes) * m / 100).min(480)) as u32,
            max_weighted_tokens: self.max_weighted_tokens * m / 100,
        }
    }
}

/// 1 実行に課す上限(ランナーに渡す。ランナーは自分の絶対上限と小さいほうを使う)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caps {
    pub max_turns: u32,
    pub max_minutes: u32,
    pub max_weighted_tokens: u64,
}

// ---- 優先度の区分 --------------------------------------------------------

/// 緊急度の区分。チケットの優先度と、他の作業をブロックしているかで決める。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PriorityClass {
    /// お客様に迷惑がかかる緊急(セキュリティ・データ漏洩など)。トークンの予算では止めない
    Emergency,
    /// 優先度が高い、または他の作業をブロックしている(ボトルネック)
    High,
    /// 通常
    Normal,
    /// 低い。予算に余裕があるときだけ
    Low,
}

impl PriorityClass {
    pub fn as_str(self) -> &'static str {
        match self {
            PriorityClass::Emergency => "emergency",
            PriorityClass::High => "high",
            PriorityClass::Normal => "normal",
            PriorityClass::Low => "low",
        }
    }
    pub fn from_str(s: &str) -> PriorityClass {
        match s {
            "emergency" => PriorityClass::Emergency,
            "high" => PriorityClass::High,
            "low" => PriorityClass::Low,
            _ => PriorityClass::Normal,
        }
    }
    /// 取得待ちの並び順(小さいほど先)。
    pub fn rank(self) -> i16 {
        match self {
            PriorityClass::Emergency => 0,
            PriorityClass::High => 1,
            PriorityClass::Normal => 2,
            PriorityClass::Low => 3,
        }
    }
    pub fn label_ja(self) -> &'static str {
        match self {
            PriorityClass::Emergency => "緊急",
            PriorityClass::High => "高",
            PriorityClass::Normal => "通常",
            PriorityClass::Low => "低",
        }
    }
}

/// チケットの優先度(`urgent` / `high` / `medium` / `low`)と、「このチケットが終わらないと進められない、
/// 未完了のチケット」の数から、区分を決める。ボトルネック(1 件以上をブロック)は、少なくとも「高」にする
/// (**緊急には自動では上げない**: 緊急は、人が優先度を urgent にしたものだけ)。
pub fn classify(priority: &str, blocks_open: u32) -> PriorityClass {
    let base = match priority {
        "urgent" => PriorityClass::Emergency,
        "high" => PriorityClass::High,
        "low" => PriorityClass::Low,
        _ => PriorityClass::Normal,
    };
    if blocks_open >= 1 && base > PriorityClass::High {
        PriorityClass::High
    } else {
        base
    }
}

/// 加重トークン = 入力 + 出力 + キャッシュ作成 + キャッシュ読み取りの 0.1 倍。
/// (キャッシュ読み取りは件数が多くても負荷が小さいので軽く数える。切り捨て)
pub fn weighted_tokens(input: i64, output: i64, cache_creation: i64, cache_read: i64) -> i64 {
    let sum = input.max(0) + output.max(0) + cache_creation.max(0) + cache_read.max(0) / 10;
    sum.max(0)
}

// ---- 起動の可否 ----------------------------------------------------------

/// 起動の可否の判定に使う、現在の状況。
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// システムで有効にされている(管理者が上限を保存し、有効にした)
    pub system_enabled: bool,
    /// 緊急停止が作動している
    pub killed: bool,
    /// このプロジェクトで「AI 自動実行」が許可されている
    pub project_enabled: bool,
    /// このプロジェクトに、ランナーが作業するリポジトリが設定されている
    pub project_repo_set: bool,
    /// そのリポジトリが、システム管理者の許可したリポジトリの一覧に入っている
    pub project_repo_allowed: bool,
    /// 起動する人の、直近 1 時間・24 時間の起動回数
    pub user_last_hour: u32,
    pub user_last_day: u32,
    /// このチケットの直近 24 時間の起動回数と、いま取得待ち・実行中の件数
    pub ticket_last_24h: u32,
    pub ticket_active: u32,
    /// 取得待ちの件数(全体)
    pub queue_len: u32,
    /// 今日(JST)の加重トークンの合計(全体・このプロジェクト)と、直近 7 日間の合計(全体)
    pub budget_system_used: u64,
    pub budget_project_used: u64,
    pub budget_system_week_used: u64,
}

/// 起動する人と、チケットの状況。
#[derive(Debug, Clone, Copy)]
pub struct KickContext {
    /// チケットの優先度などから決めた区分(`classify`)
    pub class: PriorityClass,
    /// 画面に出した警告を見たうえで、起動する人が「承知して実行する」を選んだ
    pub confirmed: bool,
    /// 起動する人が管理者か
    pub is_admin: bool,
}

/// 起動を拒否する理由。表示用の日本語を持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KickDenied {
    SystemDisabled,
    Killed,
    ProjectDisabled,
    RepoNotSet,
    /// 設定されたリポジトリが、システム管理者の許可したリポジトリの一覧に入っていない
    RepoNotAllowed,
    TicketBusy,
    UserHourly,
    UserDaily,
    TicketLimit,
    QueueFull,
    /// 予算の上限(既定は予算の 2 倍)に達した
    BudgetCeiling,
    /// 優先度が低いチケットは、予算に余裕があるときだけ実行できる
    LowPriorityBudget,
    /// 予算を超える実行の確認は、管理者だけに許可されている
    OverrideAdminOnly,
}

impl KickDenied {
    pub fn code(self) -> &'static str {
        match self {
            KickDenied::SystemDisabled => "system_disabled",
            KickDenied::Killed => "killed",
            KickDenied::ProjectDisabled => "project_disabled",
            KickDenied::RepoNotSet => "repo_not_set",
            KickDenied::RepoNotAllowed => "repo_not_allowed",
            KickDenied::TicketBusy => "ticket_busy",
            KickDenied::UserHourly => "user_hourly",
            KickDenied::UserDaily => "user_daily",
            KickDenied::TicketLimit => "ticket_limit",
            KickDenied::QueueFull => "queue_full",
            KickDenied::BudgetCeiling => "budget_ceiling",
            KickDenied::LowPriorityBudget => "low_priority_budget",
            KickDenied::OverrideAdminOnly => "override_admin_only",
        }
    }
}

impl fmt::Display for KickDenied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            KickDenied::SystemDisabled => "AI 自動実行は、管理者によってまだ有効にされていません",
            KickDenied::Killed => "AI 自動実行は、緊急停止中です。管理者に確認してください",
            KickDenied::ProjectDisabled => "このプロジェクトでは、AI 自動実行が許可されていません",
            KickDenied::RepoNotSet => "このプロジェクトに、AI が作業するリポジトリが設定されていません",
            KickDenied::RepoNotAllowed => "このリポジトリは、システム管理者が AI の作業を許可していません(管理画面の「AI 自動実行」で許可します)",
            KickDenied::TicketBusy => "このチケットは、すでに AI が対応中(または順番待ち)です",
            KickDenied::UserHourly => "1 時間あたりの起動回数の上限に達しました。しばらくしてからやり直してください",
            KickDenied::UserDaily => "1 日あたりの起動回数の上限に達しました",
            KickDenied::TicketLimit => "このチケットの 24 時間あたりの起動回数の上限に達しました",
            KickDenied::QueueFull => "順番待ちが上限に達しています。しばらくしてからやり直してください",
            KickDenied::BudgetCeiling => "AI の予算が上限(予算の 2 倍など、管理者が決めた線)に達しました。管理者に確認してください",
            KickDenied::LowPriorityBudget => "優先度が低いため、AI の予算に余裕があるときだけ実行できます。しばらくしてから、または優先度を見直してください",
            KickDenied::OverrideAdminOnly => "予算を超える実行は、管理者だけが許可できます",
        };
        f.write_str(msg)
    }
}

/// 画面に出す警告(止めない)。`code` は種類、`message` は日本語の説明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub code: &'static str,
    pub message: String,
}

/// 起動の可否の判定結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// 実行してよい(警告があれば画面に出す)
    Allow { warnings: Vec<Warning> },
    /// 警告を見て、起動する人が「承知して実行する」を選べば実行できる
    NeedsConfirm { warnings: Vec<Warning> },
    /// 拒否する
    Deny(KickDenied),
}

struct Budget {
    name_ja: &'static str,
    used: u64,
    budget: u64,
}

impl Budget {
    fn percent(&self) -> u64 {
        if self.budget == 0 {
            return u64::MAX;
        }
        self.used.saturating_mul(100) / self.budget
    }
}

/// 起動してよいか。**上限(`limits`)が無ければ必ず拒否する**(予算を決めずに有効にできない)。
///
/// 優先度の区分ごとの扱い(予算 = 日・週の予算のうち、最も使用率が高いもの):
/// - 緊急: 予算・1 人あたり・チケットあたりの回数では止めない(超過は警告)。順番待ちの上限は 2 倍まで。
/// - 高:   100% 以上は確認つきで続行。上限(予算の `hardCeilingPercent`+100 %)で拒否。
/// - 通常: 80% で警告、100% 以上は確認つきで続行。上限(予算の `hardCeilingPercent` %)で拒否。
/// - 低:   50% で警告、80% 以上は拒否(余裕があるときだけ)。
///
/// どの区分でも、緊急停止・許可・リポジトリ・二重起動の確認は先に行う。
pub fn evaluate(limits: Option<&Limits>, s: &Snapshot, ctx: &KickContext) -> Verdict {
    let Some(l) = limits else {
        return Verdict::Deny(KickDenied::SystemDisabled);
    };
    if s.killed {
        return Verdict::Deny(KickDenied::Killed);
    }
    if !s.system_enabled {
        return Verdict::Deny(KickDenied::SystemDisabled);
    }
    if !s.project_enabled {
        return Verdict::Deny(KickDenied::ProjectDisabled);
    }
    if !s.project_repo_set {
        return Verdict::Deny(KickDenied::RepoNotSet);
    }
    if !s.project_repo_allowed {
        return Verdict::Deny(KickDenied::RepoNotAllowed);
    }
    if s.ticket_active > 0 {
        return Verdict::Deny(KickDenied::TicketBusy);
    }
    let emergency = ctx.class == PriorityClass::Emergency;
    // 回数の上限: 緊急では適用しない(お客様への影響が大きい対応を、回数で止めない)
    if !emergency {
        if s.user_last_hour >= l.per_user_hour {
            return Verdict::Deny(KickDenied::UserHourly);
        }
        if s.user_last_day >= l.per_user_day {
            return Verdict::Deny(KickDenied::UserDaily);
        }
        if s.ticket_last_24h >= l.per_ticket_24h {
            return Verdict::Deny(KickDenied::TicketLimit);
        }
    }
    let queue_limit = if emergency {
        l.max_queue.saturating_mul(2)
    } else {
        l.max_queue
    };
    if s.queue_len >= queue_limit {
        return Verdict::Deny(KickDenied::QueueFull);
    }

    // 予算: 使用率が最も高いものを見る
    let budgets = [
        Budget {
            name_ja: "今日のシステム全体の予算",
            used: s.budget_system_used,
            budget: l.daily_budget_system,
        },
        Budget {
            name_ja: "今日のこのプロジェクトの予算",
            used: s.budget_project_used,
            budget: l.daily_budget_project,
        },
        Budget {
            name_ja: "今週のシステム全体の予算",
            used: s.budget_system_week_used,
            budget: l.weekly_budget_system,
        },
    ];
    let worst = budgets
        .iter()
        .max_by_key(|b| b.percent())
        .expect("予算は 3 つある");
    let pct = worst.percent();
    let ceiling = u64::from(l.hard_ceiling_percent)
        + if ctx.class == PriorityClass::High {
            100
        } else {
            0
        };

    let mut warnings: Vec<Warning> = Vec::new();
    let mut needs_confirm = false;

    match ctx.class {
        PriorityClass::Emergency => {
            warnings.push(Warning {
                code: "emergency",
                message: "緊急として実行します。トークンの予算では止まりません(使い方の急増と緊急停止では止まります)".into(),
            });
            needs_confirm = true;
            if pct >= 100 {
                warnings.push(Warning {
                    code: "budget_over",
                    message: format!(
                        "{}の {}% を使っています(緊急のため実行できます)",
                        worst.name_ja, pct
                    ),
                });
            }
        }
        PriorityClass::Low => {
            if pct >= 80 {
                return Verdict::Deny(KickDenied::LowPriorityBudget);
            }
            if pct >= 50 {
                warnings.push(near_warning(worst, pct));
            }
        }
        PriorityClass::High | PriorityClass::Normal => {
            if pct >= ceiling {
                return Verdict::Deny(KickDenied::BudgetCeiling);
            }
            if pct >= 100 {
                warnings.push(Warning {
                    code: "budget_over",
                    message: format!(
                        "{}の {}% を使っています。人の作業の利用枠に影響するおそれがあります",
                        worst.name_ja, pct
                    ),
                });
                needs_confirm = true;
            } else if pct >= 80 {
                warnings.push(near_warning(worst, pct));
            }
        }
    }

    if needs_confirm && !ctx.confirmed {
        return Verdict::NeedsConfirm { warnings };
    }
    // 予算を超える確認を管理者だけに許す設定。緊急の確認は対象外(緊急は予算の超過ではなく、緊急として実行する確認)
    if needs_confirm && !emergency && l.override_admin_only && !ctx.is_admin {
        return Verdict::Deny(KickDenied::OverrideAdminOnly);
    }
    Verdict::Allow { warnings }
}

fn near_warning(b: &Budget, pct: u64) -> Warning {
    Warning {
        code: "budget_near",
        message: format!("{}の {}% を使っています", b.name_ja, pct),
    }
}

// ---- 実行中の継続の可否(ハートビート) --------------------------------------

/// 止める理由(`t_ai_run.stop_reason` に保存する)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    Killed,
    UserCancel,
    PerRunTime,
    PerRunTokens,
    BurnRate,
    /// 予算の上限(既定は予算の 2 倍など)に達した
    BudgetCeiling,
}

impl StopReason {
    pub fn as_str(self) -> &'static str {
        match self {
            StopReason::Killed => "killed",
            StopReason::UserCancel => "user_cancel",
            StopReason::PerRunTime => "per_run_time",
            StopReason::PerRunTokens => "per_run_tokens",
            StopReason::BurnRate => "burn_rate",
            StopReason::BudgetCeiling => "budget_ceiling",
        }
    }

    /// 暴走の兆候(自動で緊急停止を作動させる理由)か。
    pub fn is_runaway(self) -> bool {
        matches!(self, StopReason::BurnRate)
    }

    /// 画面に出す停止理由(管理画面・チケットの履歴で使う)。
    #[allow(dead_code)]
    pub fn label_ja(self) -> &'static str {
        match self {
            StopReason::Killed => "緊急停止",
            StopReason::UserCancel => "取り消し",
            StopReason::PerRunTime => "1 実行の時間の上限",
            StopReason::PerRunTokens => "1 実行のトークンの上限",
            StopReason::BurnRate => "トークンの使い方が急激(暴走の疑い)",
            StopReason::BudgetCeiling => "予算の上限",
        }
    }
}

/// ハートビートの判定に使う、実行中の状況。
#[derive(Debug, Clone, Default)]
pub struct HeartbeatState {
    pub killed: bool,
    /// 起動した人などが取り消しを求めている
    pub cancel_requested: bool,
    /// 着手(取得)からの経過秒
    pub elapsed_secs: u64,
    /// この実行の、現在の加重トークン
    pub weighted_tokens: u64,
    /// 前回のハートビートからの、加重トークンの増加と経過秒(初回は 0, 0)
    pub delta_tokens: u64,
    pub delta_secs: u64,
    /// 今日の加重トークンの合計(この実行を含む。全体・プロジェクト)と、直近 7 日間の合計(全体)
    pub budget_system_used: u64,
    pub budget_project_used: u64,
    pub budget_system_week_used: u64,
    /// この実行の優先度の区分(緊急は予算で止めない)
    pub class: Option<PriorityClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeartbeatDecision {
    Continue,
    Stop(StopReason),
}

/// 増え方の計算で、あまりに短い間隔(秒)のゆらぎを拾わないための最小の窓。
const MIN_RATE_WINDOW_SECS: u64 = 30;

/// 実行を続けてよいか。止める理由は、重大なものから順に確認する。
///
/// **予算では、上限(`hardCeilingPercent`)に達するまで止めない**(業務を止めないため。超過は起動時の警告と通知で知らせる)。
/// 緊急の実行は、予算では止めない。**1 実行の上限・使い方の急増・緊急停止・取り消しは、緊急でも変わらない。**
pub fn decide_heartbeat(limits: &Limits, caps: &Caps, s: &HeartbeatState) -> HeartbeatDecision {
    if s.killed {
        return HeartbeatDecision::Stop(StopReason::Killed);
    }
    if s.cancel_requested {
        return HeartbeatDecision::Stop(StopReason::UserCancel);
    }
    if s.elapsed_secs > u64::from(caps.max_minutes) * 60 {
        return HeartbeatDecision::Stop(StopReason::PerRunTime);
    }
    if s.weighted_tokens > caps.max_weighted_tokens {
        return HeartbeatDecision::Stop(StopReason::PerRunTokens);
    }
    // 1 分あたりの増え方(サーキットブレーカー)
    let window = s.delta_secs.max(MIN_RATE_WINDOW_SECS);
    let per_min = s.delta_tokens.saturating_mul(60) / window;
    if per_min > limits.burn_rate_per_min {
        return HeartbeatDecision::Stop(StopReason::BurnRate);
    }
    // 予算の上限(緊急は対象外)
    let class = s.class.unwrap_or(PriorityClass::Normal);
    if class != PriorityClass::Emergency {
        let ceiling = u64::from(limits.hard_ceiling_percent)
            + if class == PriorityClass::High { 100 } else { 0 };
        let over =
            |used: u64, budget: u64| used.saturating_mul(100) >= budget.saturating_mul(ceiling);
        if over(s.budget_system_used, limits.daily_budget_system)
            || over(s.budget_project_used, limits.daily_budget_project)
            || over(s.budget_system_week_used, limits.weekly_budget_system)
        {
            return HeartbeatDecision::Stop(StopReason::BudgetCeiling);
        }
    }
    HeartbeatDecision::Continue
}

/// 直近の起動(新しい順の結果)が、続けて失敗していないか。失敗が続いたら緊急停止を作動させる。
/// 失敗 = `error`、または暴走の兆候で止めたもの(`aborted` で停止理由が暴走)。
pub fn should_trip_on_failures(limits: &Limits, recent_failed_flags: &[bool]) -> bool {
    let n = limits.max_consecutive_failures as usize;
    recent_failed_flags.len() >= n && recent_failed_flags.iter().take(n).all(|f| *f)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)] // テストでは、既定値から 1 項目だけ変える書き方が読みやすい
mod tests {
    use super::*;

    fn ctx(class: PriorityClass) -> KickContext {
        KickContext {
            class,
            confirmed: false,
            is_admin: false,
        }
    }
    fn confirmed(class: PriorityClass) -> KickContext {
        KickContext {
            class,
            confirmed: true,
            is_admin: false,
        }
    }

    fn ok_snapshot() -> Snapshot {
        Snapshot {
            system_enabled: true,
            killed: false,
            project_enabled: true,
            project_repo_set: true,
            project_repo_allowed: true,
            ..Default::default()
        }
    }

    /// 週の予算の使用率(%)を、指定の値にしたスナップショット。
    fn at_week_pct(l: &Limits, pct: u64) -> Snapshot {
        let mut s = ok_snapshot();
        s.budget_system_week_used = l.weekly_budget_system * pct / 100;
        s
    }

    fn is_allow(v: &Verdict) -> bool {
        matches!(v, Verdict::Allow { .. })
    }
    fn warning_codes(v: &Verdict) -> Vec<&'static str> {
        match v {
            Verdict::Allow { warnings } | Verdict::NeedsConfirm { warnings } => {
                warnings.iter().map(|w| w.code).collect()
            }
            Verdict::Deny(_) => vec![],
        }
    }

    // ---- Limits ----

    #[test]
    fn default_limits_are_valid_and_leave_room_above_measured_normal_usage() {
        let l = Limits::default();
        assert!(l.validate().is_ok());
        assert_eq!(
            (l.max_turns, l.max_minutes, l.max_weighted_tokens),
            (400, 180, 30_000_000)
        );
        assert_eq!(
            (l.daily_budget_system, l.daily_budget_project),
            (60_000_000, 40_000_000)
        );
        assert_eq!(l.weekly_budget_system, 150_000_000);
        assert_eq!(l.burn_rate_per_min, 2_000_000);
        assert_eq!((l.per_user_hour, l.per_user_day), (10, 30));
        assert_eq!(
            (l.hard_ceiling_percent, l.emergency_cap_percent),
            (200, 200)
        );
        // 実測(2026-10-07)の「普通の作業」より上であること: 中規模な作業の上位 1% は 13M・173 分・290 ターン、
        // 最大でも 16M。1 分間の最大の使い方は約 1.4M。これらを止めない値になっている
        assert!(l.max_weighted_tokens >= 2 * 16_000_000 - 2_000_000);
        assert!(l.max_minutes >= 173 && l.max_turns >= 290);
        assert!(l.burn_rate_per_min > 1_400_000);
    }

    #[test]
    fn zero_means_unlimited_and_is_rejected() {
        for mutate in [
            |l: &mut Limits| l.max_turns = 0,
            |l: &mut Limits| l.max_minutes = 0,
            |l: &mut Limits| l.max_weighted_tokens = 0,
            |l: &mut Limits| l.daily_budget_system = 0,
            |l: &mut Limits| l.daily_budget_project = 0,
            |l: &mut Limits| l.weekly_budget_system = 0,
            |l: &mut Limits| l.per_user_hour = 0,
            |l: &mut Limits| l.per_user_day = 0,
            |l: &mut Limits| l.per_ticket_24h = 0,
            |l: &mut Limits| l.max_queue = 0,
            |l: &mut Limits| l.burn_rate_per_min = 0,
            |l: &mut Limits| l.max_consecutive_failures = 0,
            |l: &mut Limits| l.hard_ceiling_percent = 0,
            |l: &mut Limits| l.emergency_cap_percent = 0,
        ] {
            let mut l = Limits::default();
            mutate(&mut l);
            assert!(l.validate().is_err(), "0 は拒否される: {l:?}");
        }
    }

    #[test]
    fn practically_unlimited_values_are_rejected() {
        let mut l = Limits::default();
        l.max_turns = 100_000;
        assert!(l.validate().is_err());
        let mut l = Limits::default();
        l.max_minutes = 100_000;
        assert!(l.validate().is_err());
        let mut l = Limits::default();
        l.max_weighted_tokens = u64::MAX;
        assert!(l.validate().is_err());
        let mut l = Limits::default();
        l.daily_budget_system = u64::MAX;
        assert!(l.validate().is_err());
        let mut l = Limits::default();
        l.hard_ceiling_percent = 100_000;
        assert!(l.validate().is_err(), "上限の線が高すぎる(実質の無制限)");
        let mut l = Limits::default();
        l.emergency_cap_percent = 100_000;
        assert!(l.validate().is_err(), "緊急の倍率が高すぎる");
    }

    #[test]
    fn emergency_caps_are_finite_even_with_max_multiplier() {
        let mut l = Limits::default();
        l.emergency_cap_percent = 500;
        l.max_weighted_tokens = 20_000_000;
        assert!(l.validate().is_ok());
        let c = l.caps_for(PriorityClass::Emergency);
        assert_eq!(c.max_weighted_tokens, 100_000_000);
        assert!(c.max_turns <= 1000 && c.max_minutes <= 480);
        l.max_weighted_tokens = 20_000_001;
        assert!(
            l.validate().is_err(),
            "緊急の上限が 100M を超える設定は拒否"
        );
    }

    #[test]
    fn budgets_must_be_consistent() {
        let mut l = Limits::default();
        l.daily_budget_project = l.daily_budget_system + 1;
        assert!(l.validate().is_err(), "プロジェクトの予算 > 全体の予算");
        let mut l = Limits::default();
        l.daily_budget_system = l.weekly_budget_system + 1;
        assert!(l.validate().is_err(), "1 日の予算 > 1 週間の予算");
        let mut l = Limits::default();
        l.max_weighted_tokens = l.daily_budget_project + 1;
        assert!(
            l.validate().is_err(),
            "1 実行の上限 > プロジェクトの日次予算"
        );
        let mut l = Limits::default();
        l.per_user_hour = l.per_user_day + 1;
        assert!(l.validate().is_err());
    }

    #[test]
    fn caps_for_non_emergency_are_the_normal_caps() {
        let l = Limits::default();
        for c in [
            PriorityClass::High,
            PriorityClass::Normal,
            PriorityClass::Low,
        ] {
            assert_eq!(l.caps_for(c), l.caps());
        }
        let e = l.caps_for(PriorityClass::Emergency);
        assert_eq!(e.max_weighted_tokens, 60_000_000, "緊急は 2 倍");
    }

    // ---- 優先度の区分 ----

    #[test]
    fn classify_follows_the_ticket_priority() {
        assert_eq!(classify("urgent", 0), PriorityClass::Emergency);
        assert_eq!(classify("high", 0), PriorityClass::High);
        assert_eq!(classify("medium", 0), PriorityClass::Normal);
        assert_eq!(classify("low", 0), PriorityClass::Low);
        assert_eq!(classify("", 0), PriorityClass::Normal, "不明な値は通常");
        assert_eq!(classify("whatever", 0), PriorityClass::Normal);
    }

    #[test]
    fn bottleneck_tickets_are_raised_to_high_but_never_to_emergency() {
        assert_eq!(classify("low", 1), PriorityClass::High);
        assert_eq!(classify("medium", 3), PriorityClass::High);
        assert_eq!(classify("high", 5), PriorityClass::High);
        assert_eq!(classify("urgent", 0), PriorityClass::Emergency);
        assert_eq!(
            classify("medium", 100),
            PriorityClass::High,
            "どれだけ詰まっていても、緊急には上げない"
        );
    }

    #[test]
    fn priority_rank_orders_the_queue() {
        let mut v = vec![
            PriorityClass::Low,
            PriorityClass::Emergency,
            PriorityClass::Normal,
            PriorityClass::High,
        ];
        v.sort_by_key(|c| c.rank());
        assert_eq!(
            v,
            vec![
                PriorityClass::Emergency,
                PriorityClass::High,
                PriorityClass::Normal,
                PriorityClass::Low
            ]
        );
        for c in v {
            assert_eq!(PriorityClass::from_str(c.as_str()), c);
            assert!(!c.label_ja().is_empty());
        }
    }

    // ---- weighted_tokens ----

    #[test]
    fn weighted_tokens_counts_cache_reads_lightly() {
        assert_eq!(weighted_tokens(100, 50, 30, 1000), 100 + 50 + 30 + 100);
        assert_eq!(weighted_tokens(0, 0, 0, 9), 0, "切り捨て");
        assert_eq!(weighted_tokens(-5, -5, -5, -50), 0, "負の値は 0 として扱う");
    }

    // ---- evaluate: 共通の確認 ----

    #[test]
    fn a_normal_request_is_allowed_without_warnings() {
        let l = Limits::default();
        let v = evaluate(Some(&l), &ok_snapshot(), &ctx(PriorityClass::Normal));
        assert_eq!(v, Verdict::Allow { warnings: vec![] });
    }

    #[test]
    fn refuses_without_saved_limits_for_every_class() {
        for c in [
            PriorityClass::Emergency,
            PriorityClass::High,
            PriorityClass::Normal,
            PriorityClass::Low,
        ] {
            assert_eq!(
                evaluate(None, &ok_snapshot(), &confirmed(c)),
                Verdict::Deny(KickDenied::SystemDisabled),
                "上限が未設定なら、緊急でも起動できない(予算を決めずに有効にできない)"
            );
        }
    }

    #[test]
    fn kill_switch_and_disabled_flags_stop_every_class_including_emergency() {
        let l = Limits::default();
        for c in [
            PriorityClass::Emergency,
            PriorityClass::High,
            PriorityClass::Normal,
            PriorityClass::Low,
        ] {
            let mut s = ok_snapshot();
            s.killed = true;
            assert_eq!(
                evaluate(Some(&l), &s, &confirmed(c)),
                Verdict::Deny(KickDenied::Killed)
            );
            let mut s = ok_snapshot();
            s.system_enabled = false;
            assert_eq!(
                evaluate(Some(&l), &s, &confirmed(c)),
                Verdict::Deny(KickDenied::SystemDisabled)
            );
            let mut s = ok_snapshot();
            s.project_enabled = false;
            assert_eq!(
                evaluate(Some(&l), &s, &confirmed(c)),
                Verdict::Deny(KickDenied::ProjectDisabled)
            );
            let mut s = ok_snapshot();
            s.project_repo_set = false;
            assert_eq!(
                evaluate(Some(&l), &s, &confirmed(c)),
                Verdict::Deny(KickDenied::RepoNotSet)
            );
            let mut s = ok_snapshot();
            s.project_repo_allowed = false;
            assert_eq!(
                evaluate(Some(&l), &s, &confirmed(c)),
                Verdict::Deny(KickDenied::RepoNotAllowed)
            );
            let mut s = ok_snapshot();
            s.ticket_active = 1;
            assert_eq!(
                evaluate(Some(&l), &s, &confirmed(c)),
                Verdict::Deny(KickDenied::TicketBusy),
                "緊急でも、同じチケットの二重起動はしない"
            );
        }
    }

    #[test]
    fn kill_switch_beats_everything_else() {
        let l = Limits::default();
        let mut s = ok_snapshot();
        s.killed = true;
        s.system_enabled = false;
        assert_eq!(
            evaluate(Some(&l), &s, &ctx(PriorityClass::Normal)),
            Verdict::Deny(KickDenied::Killed),
            "無効でも、まず緊急停止と伝える"
        );
    }

    // ---- evaluate: 回数 ----

    #[test]
    fn rate_limits_apply_to_non_emergency_exactly_at_the_limit() {
        let l = Limits::default();
        for class in [PriorityClass::High, PriorityClass::Normal] {
            let mut s = ok_snapshot();
            s.user_last_hour = l.per_user_hour - 1;
            assert!(is_allow(&evaluate(Some(&l), &s, &ctx(class))));
            s.user_last_hour = l.per_user_hour;
            assert_eq!(
                evaluate(Some(&l), &s, &ctx(class)),
                Verdict::Deny(KickDenied::UserHourly)
            );

            let mut s = ok_snapshot();
            s.user_last_day = l.per_user_day;
            assert_eq!(
                evaluate(Some(&l), &s, &ctx(class)),
                Verdict::Deny(KickDenied::UserDaily)
            );

            let mut s = ok_snapshot();
            s.ticket_last_24h = l.per_ticket_24h;
            assert_eq!(
                evaluate(Some(&l), &s, &ctx(class)),
                Verdict::Deny(KickDenied::TicketLimit)
            );

            let mut s = ok_snapshot();
            s.queue_len = l.max_queue;
            assert_eq!(
                evaluate(Some(&l), &s, &ctx(class)),
                Verdict::Deny(KickDenied::QueueFull)
            );
        }
    }

    #[test]
    fn emergency_is_not_stopped_by_per_user_or_per_ticket_counts() {
        let l = Limits::default();
        let mut s = ok_snapshot();
        s.user_last_hour = 999;
        s.user_last_day = 999;
        s.ticket_last_24h = 999;
        let v = evaluate(Some(&l), &s, &confirmed(PriorityClass::Emergency));
        assert!(is_allow(&v), "緊急は、回数の上限では止めない: {v:?}");
    }

    #[test]
    fn emergency_queue_limit_is_double_but_still_finite() {
        let l = Limits::default();
        let mut s = ok_snapshot();
        s.queue_len = l.max_queue; // 通常ならここで拒否
        assert!(is_allow(&evaluate(
            Some(&l),
            &s,
            &confirmed(PriorityClass::Emergency)
        )));
        s.queue_len = l.max_queue * 2;
        assert_eq!(
            evaluate(Some(&l), &s, &confirmed(PriorityClass::Emergency)),
            Verdict::Deny(KickDenied::QueueFull)
        );
    }

    // ---- evaluate: 予算(通常) ----

    #[test]
    fn normal_budget_warns_at_80_confirms_at_100_and_refuses_at_ceiling() {
        let l = Limits::default();
        // 79%: 警告なし
        assert_eq!(
            evaluate(Some(&l), &at_week_pct(&l, 79), &ctx(PriorityClass::Normal)),
            Verdict::Allow { warnings: vec![] }
        );
        // 80%: 警告(止めない)
        let v = evaluate(Some(&l), &at_week_pct(&l, 80), &ctx(PriorityClass::Normal));
        assert!(is_allow(&v));
        assert_eq!(warning_codes(&v), vec!["budget_near"]);
        // 100%: 確認が必要
        let v = evaluate(Some(&l), &at_week_pct(&l, 100), &ctx(PriorityClass::Normal));
        assert!(matches!(v, Verdict::NeedsConfirm { .. }));
        assert_eq!(warning_codes(&v), vec!["budget_over"]);
        // 確認すれば実行できる(業務を止めない)
        let v = evaluate(
            Some(&l),
            &at_week_pct(&l, 150),
            &confirmed(PriorityClass::Normal),
        );
        assert!(is_allow(&v), "予算を超えても、承知して実行できる: {v:?}");
        // 上限(既定は 2 倍)で拒否
        assert_eq!(
            evaluate(
                Some(&l),
                &at_week_pct(&l, 200),
                &confirmed(PriorityClass::Normal)
            ),
            Verdict::Deny(KickDenied::BudgetCeiling)
        );
    }

    #[test]
    fn the_worst_budget_is_the_one_that_counts() {
        let l = Limits::default();
        let mut s = ok_snapshot();
        s.budget_project_used = l.daily_budget_project; // プロジェクトの日次が 100%
        let v = evaluate(Some(&l), &s, &ctx(PriorityClass::Normal));
        assert!(matches!(v, Verdict::NeedsConfirm { .. }));
        let mut s = ok_snapshot();
        s.budget_system_used = l.daily_budget_system * 2;
        assert_eq!(
            evaluate(Some(&l), &s, &confirmed(PriorityClass::Normal)),
            Verdict::Deny(KickDenied::BudgetCeiling)
        );
    }

    #[test]
    fn override_can_be_limited_to_admins() {
        let mut l = Limits::default();
        l.override_admin_only = true;
        let s = at_week_pct(&l, 120);
        let member = KickContext {
            class: PriorityClass::Normal,
            confirmed: true,
            is_admin: false,
        };
        assert_eq!(
            evaluate(Some(&l), &s, &member),
            Verdict::Deny(KickDenied::OverrideAdminOnly)
        );
        let admin = KickContext {
            is_admin: true,
            ..member
        };
        assert!(is_allow(&evaluate(Some(&l), &s, &admin)));
        // 予算を超えていなければ、一般のメンバーも起動できる
        assert!(is_allow(&evaluate(
            Some(&l),
            &at_week_pct(&l, 50),
            &ctx(PriorityClass::Normal)
        )));
    }

    #[test]
    fn budget_arithmetic_does_not_overflow() {
        let l = Limits::default();
        let mut s = ok_snapshot();
        s.budget_system_used = u64::MAX;
        assert_eq!(
            evaluate(Some(&l), &s, &confirmed(PriorityClass::Normal)),
            Verdict::Deny(KickDenied::BudgetCeiling)
        );
    }

    // ---- evaluate: 予算(高・低・緊急) ----

    #[test]
    fn high_priority_has_a_higher_ceiling_than_normal() {
        let l = Limits::default();
        let s = at_week_pct(&l, 250);
        assert_eq!(
            evaluate(Some(&l), &s, &confirmed(PriorityClass::Normal)),
            Verdict::Deny(KickDenied::BudgetCeiling)
        );
        assert!(is_allow(&evaluate(
            Some(&l),
            &s,
            &confirmed(PriorityClass::High)
        )));
        assert_eq!(
            evaluate(
                Some(&l),
                &at_week_pct(&l, 300),
                &confirmed(PriorityClass::High)
            ),
            Verdict::Deny(KickDenied::BudgetCeiling),
            "高にも上限はある"
        );
    }

    #[test]
    fn low_priority_runs_only_when_there_is_room() {
        let l = Limits::default();
        let v = evaluate(Some(&l), &at_week_pct(&l, 49), &ctx(PriorityClass::Low));
        assert_eq!(v, Verdict::Allow { warnings: vec![] });
        let v = evaluate(Some(&l), &at_week_pct(&l, 50), &ctx(PriorityClass::Low));
        assert_eq!(warning_codes(&v), vec!["budget_near"]);
        assert_eq!(
            evaluate(
                Some(&l),
                &at_week_pct(&l, 80),
                &confirmed(PriorityClass::Low)
            ),
            Verdict::Deny(KickDenied::LowPriorityBudget),
            "確認しても、低は予算に余裕がないと実行できない"
        );
    }

    #[test]
    fn emergency_is_never_refused_by_budget_but_always_needs_an_explicit_ok() {
        let l = Limits::default();
        // 予算が 10 倍でも、緊急は実行できる(確認は必要)
        let s = at_week_pct(&l, 1000);
        let v = evaluate(Some(&l), &s, &ctx(PriorityClass::Emergency));
        assert!(matches!(v, Verdict::NeedsConfirm { .. }));
        assert!(warning_codes(&v).contains(&"emergency"));
        assert!(warning_codes(&v).contains(&"budget_over"));
        let v = evaluate(Some(&l), &s, &confirmed(PriorityClass::Emergency));
        assert!(is_allow(&v), "緊急は予算では止めない: {v:?}");
        // 予算が空でも、緊急には「緊急として実行」の確認が付く
        let v = evaluate(Some(&l), &ok_snapshot(), &ctx(PriorityClass::Emergency));
        assert!(matches!(v, Verdict::NeedsConfirm { .. }));
        // 「予算を超える確認は管理者だけ」の設定でも、緊急は一般のメンバーが起動できる
        let mut l2 = Limits::default();
        l2.override_admin_only = true;
        let v = evaluate(
            Some(&l2),
            &at_week_pct(&l2, 1000),
            &confirmed(PriorityClass::Emergency),
        );
        assert!(is_allow(&v));
    }

    #[test]
    fn every_denial_has_a_code_and_a_japanese_message() {
        for d in [
            KickDenied::SystemDisabled,
            KickDenied::Killed,
            KickDenied::ProjectDisabled,
            KickDenied::RepoNotSet,
            KickDenied::TicketBusy,
            KickDenied::UserHourly,
            KickDenied::UserDaily,
            KickDenied::TicketLimit,
            KickDenied::QueueFull,
            KickDenied::BudgetCeiling,
            KickDenied::LowPriorityBudget,
            KickDenied::OverrideAdminOnly,
        ] {
            assert!(!d.code().is_empty());
            assert!(!d.to_string().is_empty());
        }
    }

    // ---- decide_heartbeat ----

    fn hb(class: PriorityClass) -> HeartbeatState {
        HeartbeatState {
            elapsed_secs: 60,
            weighted_tokens: 10_000,
            delta_tokens: 5_000,
            delta_secs: 30,
            budget_system_used: 10_000,
            budget_project_used: 10_000,
            budget_system_week_used: 10_000,
            class: Some(class),
            ..Default::default()
        }
    }

    #[test]
    fn heartbeat_continues_in_normal_conditions() {
        let l = Limits::default();
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &hb(PriorityClass::Normal)),
            HeartbeatDecision::Continue
        );
    }

    #[test]
    fn heartbeat_stops_on_kill_and_cancel_first() {
        let l = Limits::default();
        let mut s = hb(PriorityClass::Emergency);
        s.killed = true;
        s.cancel_requested = true;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Stop(StopReason::Killed),
            "緊急停止は緊急にも効く"
        );
        s.killed = false;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Stop(StopReason::UserCancel)
        );
    }

    #[test]
    fn heartbeat_stops_when_per_run_time_or_tokens_exceeded() {
        let l = Limits::default();
        let caps = l.caps();
        let mut s = hb(PriorityClass::Normal);
        s.elapsed_secs = u64::from(caps.max_minutes) * 60;
        assert_eq!(
            decide_heartbeat(&l, &caps, &s),
            HeartbeatDecision::Continue,
            "ちょうどは継続"
        );
        s.elapsed_secs += 1;
        assert_eq!(
            decide_heartbeat(&l, &caps, &s),
            HeartbeatDecision::Stop(StopReason::PerRunTime)
        );

        let mut s = hb(PriorityClass::Normal);
        s.weighted_tokens = caps.max_weighted_tokens;
        assert_eq!(decide_heartbeat(&l, &caps, &s), HeartbeatDecision::Continue);
        s.weighted_tokens += 1;
        assert_eq!(
            decide_heartbeat(&l, &caps, &s),
            HeartbeatDecision::Stop(StopReason::PerRunTokens)
        );
    }

    #[test]
    fn emergency_runs_get_bigger_caps_but_are_still_stopped_at_them() {
        let l = Limits::default();
        let caps = l.caps_for(PriorityClass::Emergency);
        let mut s = hb(PriorityClass::Emergency);
        s.weighted_tokens = l.max_weighted_tokens + 1; // 通常の上限は超えている
        assert_eq!(decide_heartbeat(&l, &caps, &s), HeartbeatDecision::Continue);
        s.weighted_tokens = caps.max_weighted_tokens + 1;
        assert_eq!(
            decide_heartbeat(&l, &caps, &s),
            HeartbeatDecision::Stop(StopReason::PerRunTokens),
            "緊急にも、有限の上限がある"
        );
    }

    #[test]
    fn heartbeat_uses_the_smaller_caps_the_caller_passes() {
        let l = Limits::default();
        let caps = Caps {
            max_turns: 10,
            max_minutes: 1,
            max_weighted_tokens: 1_000,
        };
        let mut s = hb(PriorityClass::Normal);
        s.weighted_tokens = 1_001;
        assert_eq!(
            decide_heartbeat(&l, &caps, &s),
            HeartbeatDecision::Stop(StopReason::PerRunTokens)
        );
    }

    #[test]
    fn heartbeat_detects_runaway_burn_rate_for_every_class_including_emergency() {
        let l = Limits::default();
        for class in [PriorityClass::Emergency, PriorityClass::Normal] {
            let mut s = hb(class);
            // 30 秒で 1.1M = 2.2M/分 > 2M/分
            s.delta_tokens = 1_100_000;
            s.delta_secs = 30;
            assert_eq!(
                decide_heartbeat(&l, &l.caps_for(class), &s),
                HeartbeatDecision::Stop(StopReason::BurnRate),
                "{class:?}"
            );
            // ちょうど 2M/分(30 秒で 1M)は許容
            s.delta_tokens = 1_000_000;
            assert_eq!(
                decide_heartbeat(&l, &l.caps_for(class), &s),
                HeartbeatDecision::Continue
            );
        }
    }

    #[test]
    fn burn_rate_window_is_at_least_30_seconds() {
        let l = Limits::default();
        let mut s = hb(PriorityClass::Normal);
        // 1 秒で 400k は、窓を 30 秒として 800k/分 → 継続(ゆらぎで止めない)
        s.delta_tokens = 400_000;
        s.delta_secs = 1;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Continue
        );
        // 1 秒で 1.1M は 2.2M/分 → 止める
        s.delta_tokens = 1_100_000;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Stop(StopReason::BurnRate)
        );
    }

    #[test]
    fn heartbeat_does_not_stop_on_budget_until_the_ceiling() {
        let l = Limits::default();
        // 予算の 150%(上限の 200% より下)では、実行中のものは止めない(業務を止めない)
        let mut s = hb(PriorityClass::Normal);
        s.budget_system_week_used = l.weekly_budget_system * 3 / 2;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Continue
        );
        // 上限(200%)に達したら止める
        s.budget_system_week_used = l.weekly_budget_system * 2;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Stop(StopReason::BudgetCeiling)
        );
        let mut s = hb(PriorityClass::Normal);
        s.budget_project_used = l.daily_budget_project * 2;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Stop(StopReason::BudgetCeiling)
        );
        let mut s = hb(PriorityClass::Normal);
        s.budget_system_used = l.daily_budget_system * 2;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Stop(StopReason::BudgetCeiling)
        );
    }

    #[test]
    fn heartbeat_budget_ceiling_depends_on_the_class() {
        let l = Limits::default();
        let mut high = hb(PriorityClass::High);
        high.budget_system_week_used = l.weekly_budget_system * 2;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &high),
            HeartbeatDecision::Continue,
            "高は 300% まで"
        );
        high.budget_system_week_used = l.weekly_budget_system * 3;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &high),
            HeartbeatDecision::Stop(StopReason::BudgetCeiling)
        );
        let mut em = hb(PriorityClass::Emergency);
        em.budget_system_week_used = l.weekly_budget_system * 100;
        em.budget_project_used = l.daily_budget_project * 100;
        em.budget_system_used = l.daily_budget_system * 100;
        assert_eq!(
            decide_heartbeat(&l, &l.caps_for(PriorityClass::Emergency), &em),
            HeartbeatDecision::Continue,
            "緊急は、予算では止めない"
        );
    }

    #[test]
    fn missing_class_is_treated_as_normal() {
        let l = Limits::default();
        let mut s = hb(PriorityClass::Normal);
        s.class = None;
        s.budget_system_week_used = l.weekly_budget_system * 2;
        assert_eq!(
            decide_heartbeat(&l, &l.caps(), &s),
            HeartbeatDecision::Stop(StopReason::BudgetCeiling),
            "区分が不明なときは、緊急として扱わない(安全側)"
        );
    }

    #[test]
    fn only_burn_rate_is_a_runaway_signal() {
        assert!(StopReason::BurnRate.is_runaway());
        for r in [
            StopReason::Killed,
            StopReason::UserCancel,
            StopReason::PerRunTime,
            StopReason::PerRunTokens,
            StopReason::BudgetCeiling,
        ] {
            assert!(!r.is_runaway());
            assert!(!r.as_str().is_empty() && !r.label_ja().is_empty());
        }
    }

    #[test]
    fn breaker_trips_only_on_consecutive_recent_failures() {
        let l = Limits::default(); // 3 回
        assert!(!should_trip_on_failures(&l, &[]));
        assert!(
            !should_trip_on_failures(&l, &[true, true]),
            "件数が足りない"
        );
        assert!(should_trip_on_failures(&l, &[true, true, true]));
        assert!(
            !should_trip_on_failures(&l, &[true, false, true]),
            "途中に成功があれば続けていない"
        );
        assert!(
            should_trip_on_failures(&l, &[true, true, true, false]),
            "古い成功は関係ない"
        );
    }
}
