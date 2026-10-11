//! KB エンジン設定

use super::runner::Runner;
use std::sync::OnceLock;
use std::time::Duration;

/// KB エンジンの有効/無効
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KbEngine {
    Off,
    Rules,
}

impl KbEngine {
    /// 環境変数から解析。大文字小文字は区別しない。
    /// 不正値のときは warn を1回出して Off に倒す。
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            None | Some("") => KbEngine::Off,
            Some(v) => {
                let lower = v.to_lowercase();
                match lower.as_str() {
                    "off" => KbEngine::Off,
                    "rules" => KbEngine::Rules,
                    _ => {
                        tracing::warn!("unknown SENN_KB_ENGINE value: {}, defaulting to off", v);
                        KbEngine::Off
                    }
                }
            }
        }
    }
}

/// クローズ時の KB 判定モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KbCloseMode {
    /// 警告のみ(UI で表示)
    Warn,
    /// ブロック(クローズ不可)
    Block,
}

impl KbCloseMode {
    /// 環境変数から解析。大文字小文字は区別しない。
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            None | Some("") => KbCloseMode::Warn,
            Some(v) => {
                let lower = v.to_lowercase();
                match lower.as_str() {
                    "warn" => KbCloseMode::Warn,
                    "block" => KbCloseMode::Block,
                    _ => KbCloseMode::Warn,
                }
            }
        }
    }
}

/// KB エンジンの共有ランナー
///
/// Semaphore をプロセス全体で1つにするため、OnceLock で管理する。
/// 最初の呼び出しの設定で Runner を作成し、以降は同じインスタンスを返す。
pub fn shared_runner(cfg: &crate::config::AppConfig) -> &'static Runner {
    static RUNNER: OnceLock<Runner> = OnceLock::new();
    RUNNER.get_or_init(|| {
        Runner::new(
            cfg.kb_scryer_bin.clone(),
            Duration::from_millis(cfg.kb_timeout_ms),
            cfg.kb_max_procs,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kb_engine_parse() {
        assert_eq!(KbEngine::parse(None), KbEngine::Off);
        assert_eq!(KbEngine::parse(Some("")), KbEngine::Off);
        assert_eq!(KbEngine::parse(Some("off")), KbEngine::Off);
        assert_eq!(KbEngine::parse(Some("OFF")), KbEngine::Off);
        assert_eq!(KbEngine::parse(Some("Off")), KbEngine::Off);
        assert_eq!(KbEngine::parse(Some("rules")), KbEngine::Rules);
        assert_eq!(KbEngine::parse(Some("RULES")), KbEngine::Rules);
        assert_eq!(KbEngine::parse(Some("Rules")), KbEngine::Rules);
        // 不正値は warn して Off に倒す
        assert_eq!(KbEngine::parse(Some("invalid")), KbEngine::Off);
    }

    #[test]
    fn test_kb_close_mode_parse() {
        assert_eq!(KbCloseMode::parse(None), KbCloseMode::Warn);
        assert_eq!(KbCloseMode::parse(Some("")), KbCloseMode::Warn);
        assert_eq!(KbCloseMode::parse(Some("warn")), KbCloseMode::Warn);
        assert_eq!(KbCloseMode::parse(Some("WARN")), KbCloseMode::Warn);
        assert_eq!(KbCloseMode::parse(Some("Warn")), KbCloseMode::Warn);
        assert_eq!(KbCloseMode::parse(Some("block")), KbCloseMode::Block);
        assert_eq!(KbCloseMode::parse(Some("BLOCK")), KbCloseMode::Block);
        assert_eq!(KbCloseMode::parse(Some("Block")), KbCloseMode::Block);
        // 不正値も warn 既定値に倒す
        assert_eq!(KbCloseMode::parse(Some("invalid")), KbCloseMode::Warn);
    }
}
