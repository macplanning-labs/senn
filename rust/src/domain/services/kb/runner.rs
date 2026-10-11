//! Scryer Prolog ランタイム実行
//!
//! 1リクエスト1プロセスの短命起動。待機中のメモリ消費は 0。
//!
//! 安全側の原則:
//! - Scryer はエラー・警告を **stdout** に出し終了コードも 0 のままにするため(設計書 §11.1)、
//!   ここでは「実行できたか・上限内か」だけを保証し、出力の意味の検査は `parse` に任せる。
//! - 時間・出力量・同時数・一時ファイルの権限を必ず制限し、どの経路でも一時ファイルを消す。

use super::KbError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::Semaphore;

const STDOUT_LIMIT: usize = 64 * 1024;
const STDERR_LIMIT: usize = 16 * 1024;

static TEMP_FILE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 一時ファイルを必ず削除するガード
struct TempFileGuard {
    path: PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Scryer Prolog 実行ランナー
pub struct Runner {
    bin: String,
    timeout: Duration,
    semaphore: Arc<Semaphore>,
    tmp_dir: PathBuf,
}

impl Runner {
    /// 新しいランナーを作成
    ///
    /// - `bin`: Scryer Prolog の実行ファイル(名前またはパス)
    /// - `timeout`: 1回の実行(起動〜終了)の時間上限
    /// - `max_procs`: 同時起動数の上限
    pub fn new(bin: impl Into<String>, timeout: Duration, max_procs: usize) -> Self {
        Runner {
            bin: bin.into(),
            timeout,
            semaphore: Arc::new(Semaphore::new(max_procs)),
            tmp_dir: std::env::temp_dir(),
        }
    }

    /// 一時ファイルの置き場を変える(既定は OS の一時ディレクトリ)
    pub fn with_tmp_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.tmp_dir = dir.into();
        self
    }

    /// ルールと事実を連結して実行し、stdout を返す
    ///
    /// - `rules`: Prolog ルール本文
    /// - `facts_text`: `Facts::finish()` の出力(`facts_count(N).` を含む)
    /// - `goal`: 実行するゴール名。`^[a-z][a-z0-9_]*$` のみ許可
    ///
    /// エラー: `InvalidInput`(goal 不正) / `Busy`(同時数超過) / `Timeout` /
    /// `TooLarge`(stdout が 64KB 超) / `ProcessFailed`(起動失敗・非0終了・stderr 出力あり)
    pub async fn run(&self, rules: &str, facts_text: &str, goal: &str) -> Result<String, KbError> {
        if !valid_goal(goal) {
            return Err(KbError::InvalidInput(
                "goal must match ^[a-z][a-z0-9_]*$".to_string(),
            ));
        }

        // 待たずに断る(詰まったリクエストが積み上がってメモリを食わないように)
        let _permit = self
            .semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| KbError::Busy)?;

        // ガードは関数末尾(成功・失敗・タイムアウトのどの経路でも)で一時ファイルを消す
        let temp = self.create_temp_file(rules, facts_text)?;

        // シェルは経由しない。チケット由来の文字列は引数に載せない(goal は固定名のみ)
        let mut child = Command::new(&self.bin)
            .arg("-f")
            .arg("--no-add-history")
            .arg(&temp.path)
            .arg("-g")
            .arg(goal)
            .arg("-g")
            .arg("halt")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| KbError::ProcessFailed(format!("spawn failed: {e}")))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let work = async {
            // stdout と stderr は並行して読む(片方が詰まって子が固まるのを防ぐ)
            let (out, err) = tokio::join!(
                read_limited(stdout, STDOUT_LIMIT),
                read_limited(stderr, STDERR_LIMIT)
            );
            let (out, out_truncated) =
                out.map_err(|e| KbError::ProcessFailed(format!("read stdout failed: {e}")))?;
            let (err, _) =
                err.map_err(|e| KbError::ProcessFailed(format!("read stderr failed: {e}")))?;
            let status = child
                .wait()
                .await
                .map_err(|e| KbError::ProcessFailed(format!("wait failed: {e}")))?;

            let err_text = String::from_utf8_lossy(&err);
            if !status.success() {
                let head: String = err_text.chars().take(200).collect();
                return Err(KbError::ProcessFailed(format!(
                    "exit status {status}: {head}"
                )));
            }
            if !err_text.trim().is_empty() {
                let head: String = err_text.chars().take(200).collect();
                return Err(KbError::ProcessFailed(format!("stderr: {head}")));
            }
            if out_truncated {
                return Err(KbError::TooLarge);
            }
            Ok(String::from_utf8_lossy(&out).into_owned())
        };

        match tokio::time::timeout(self.timeout, work).await {
            Ok(r) => r,
            Err(_) => {
                let _ = child.start_kill();
                Err(KbError::Timeout)
            }
        }
    }

    /// ルールと事実を 0600 の一時ファイルに書く
    fn create_temp_file(&self, rules: &str, facts: &str) -> Result<TempFileGuard, KbError> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let seq = TEMP_FILE_SEQ.fetch_add(1, Ordering::SeqCst);
        let path = self
            .tmp_dir
            .join(format!("kb-{}-{}-{}.pl", std::process::id(), nanos, seq));

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| KbError::ProcessFailed(format!("create temp file failed: {e}")))?;
        // 書き込みに失敗してもファイルを残さないよう、先にガードを作る
        let guard = TempFileGuard { path };
        file.write_all(rules.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.write_all(facts.as_bytes()))
            .map_err(|e| KbError::ProcessFailed(format!("write temp file failed: {e}")))?;
        Ok(guard)
    }
}

fn valid_goal(goal: &str) -> bool {
    let mut chars = goal.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// 先頭 `limit` バイトだけ保持し、超過分は読み捨てる(子をブロックさせないため EOF まで読む)。
/// 戻り値の bool は「上限を超えた」。
async fn read_limited<R: AsyncRead + Unpin>(
    reader: Option<R>,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let Some(mut reader) = reader else {
        return Ok((Vec::new(), false));
    };
    let mut buf = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 8192];
    loop {
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        let room = limit.saturating_sub(buf.len());
        let take = n.min(room);
        buf.extend_from_slice(&chunk[..take]);
        if take < n {
            truncated = true;
        }
    }
    Ok((buf, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// テスト専用の作業ディレクトリ(他テストと衝突せず、終了時に丸ごと消す)
    struct Sandbox {
        dir: PathBuf,
    }

    impl Sandbox {
        fn new() -> Self {
            let n = TEMP_FILE_SEQ.fetch_add(1, Ordering::SeqCst);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let dir = std::env::temp_dir()
                .join(format!("kb-runner-test-{}-{nanos}-{n}", std::process::id()));
            fs::create_dir_all(&dir).expect("create sandbox");
            Sandbox { dir }
        }

        /// 実行可能なダミースクリプトを作り、そのパスを返す
        fn script(&self, body: &str) -> String {
            use std::os::unix::fs::PermissionsExt;
            let path = self.dir.join("dummy.sh");
            fs::write(&path, body).expect("write script");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
            path.to_string_lossy().into_owned()
        }

        fn runner(&self, body: &str, timeout_ms: u64, max_procs: usize) -> Runner {
            let work = self.dir.join("work");
            fs::create_dir_all(&work).expect("create work dir");
            Runner::new(
                self.script(body),
                Duration::from_millis(timeout_ms),
                max_procs,
            )
            .with_tmp_dir(work)
        }

        fn leftover_files(&self) -> usize {
            fs::read_dir(self.dir.join("work"))
                .map(|d| d.count())
                .unwrap_or(0)
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    /// 書き込んだばかりのスクリプトを、別スレッドの fork が一瞬掴むと exec が
    /// ETXTBSY(Text file busy)で失敗することがある(並列テストでの既知の競合)。
    /// 本番コードの問題ではないので、その場合だけ数回やり直す。
    async fn run_goal(r: &Runner, rules: &str, facts: &str, goal: &str) -> Result<String, KbError> {
        let mut last = Err(KbError::Busy);
        for _ in 0..8 {
            last = r.run(rules, facts, goal).await;
            match &last {
                Err(KbError::ProcessFailed(m)) if m.contains("Text file busy") => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                _ => break,
            }
        }
        last
    }

    async fn run(r: &Runner) -> Result<String, KbError> {
        run_goal(r, "rules.", "facts_count(0).\n", "run_close").await
    }

    #[tokio::test]
    async fn invalid_goal_is_rejected() {
        let sb = Sandbox::new();
        let r = sb.runner("#!/bin/sh\necho OK\n", 10000, 1);
        for goal in [
            "Test",
            "test-goal",
            "",
            "a b",
            "1abc",
            "run_close; halt",
            "x\n",
        ] {
            let res = r.run("r.", "facts_count(0).\n", goal).await;
            assert!(
                matches!(res, Err(KbError::InvalidInput(_))),
                "goal {goal:?} must be rejected"
            );
        }
        assert_eq!(sb.leftover_files(), 0);
    }

    #[tokio::test]
    async fn success_returns_stdout_and_cleans_up() {
        let sb = Sandbox::new();
        let r = sb.runner("#!/bin/sh\necho 'OK|X'\necho 'END|1'\n", 10000, 1);
        let out = run(&r).await.expect("success");
        assert_eq!(out, "OK|X\nEND|1\n");
        assert_eq!(sb.leftover_files(), 0, "temp file must be removed");
    }

    #[tokio::test]
    async fn temp_file_contains_rules_then_facts_and_is_0600() {
        let sb = Sandbox::new();
        // 実行中に引数(= 一時ファイル)の内容と権限を stdout に写すスクリプト
        let body = "#!/bin/sh\nf=\"$3\"\nstat_mode=$(ls -l \"$f\" | cut -c1-10)\necho \"$stat_mode\"\ncat \"$f\"\n";
        let r = sb.runner(body, 10000, 1);
        let out = run_goal(
            &r,
            "RULES_BODY.",
            "fact_a(1).\nfacts_count(1).\n",
            "run_close",
        )
        .await
        .expect("success");
        assert!(out.starts_with("-rw-------"), "mode must be 0600: {out}");
        assert!(
            out.contains("RULES_BODY.\nfact_a(1)."),
            "rules then facts: {out}"
        );
        assert_eq!(sb.leftover_files(), 0);
    }

    #[tokio::test]
    async fn arguments_are_fixed_flags_file_and_goals() {
        let sb = Sandbox::new();
        let r = sb.runner(
            "#!/bin/sh\nfor a in \"$@\"; do echo \"ARG:$a\"; done\n",
            10000,
            1,
        );
        let out = run(&r).await.expect("success");
        let args: Vec<&str> = out.lines().collect();
        assert_eq!(args[0], "ARG:-f");
        assert_eq!(args[1], "ARG:--no-add-history");
        assert!(args[2].starts_with("ARG:") && args[2].ends_with(".pl"));
        assert_eq!(
            &args[3..],
            ["ARG:-g", "ARG:run_close", "ARG:-g", "ARG:halt"]
        );
    }

    #[tokio::test]
    async fn nonzero_exit_is_error() {
        let sb = Sandbox::new();
        let r = sb.runner("#!/bin/sh\necho 'OK|X'\nexit 3\n", 10000, 1);
        let res = run(&r).await;
        assert!(matches!(res, Err(KbError::ProcessFailed(_))), "{res:?}");
        assert_eq!(sb.leftover_files(), 0);
    }

    #[tokio::test]
    async fn stderr_output_is_error() {
        let sb = Sandbox::new();
        let r = sb.runner("#!/bin/sh\necho 'OK|X'\necho 'boom' >&2\n", 10000, 1);
        let res = run(&r).await;
        assert!(matches!(res, Err(KbError::ProcessFailed(_))), "{res:?}");
    }

    #[tokio::test]
    async fn huge_stdout_is_too_large() {
        let sb = Sandbox::new();
        let r = sb.runner(
            "#!/bin/sh\nhead -c 200000 /dev/zero | tr '\\000' 'a'\n",
            30_000,
            1,
        );
        let res = run(&r).await;
        assert_eq!(res, Err(KbError::TooLarge));
        assert_eq!(sb.leftover_files(), 0);
    }

    #[tokio::test]
    async fn stdout_exactly_at_limit_is_accepted() {
        let sb = Sandbox::new();
        let r = sb.runner(
            "#!/bin/sh\nhead -c 65536 /dev/zero | tr '\\000' 'a'\n",
            30_000,
            1,
        );
        let out = run(&r).await.expect("exactly the limit is fine");
        assert_eq!(out.len(), 65536);
    }

    #[tokio::test]
    async fn huge_stderr_does_not_deadlock() {
        let sb = Sandbox::new();
        // stderr を 200KB 出してから stdout を出す。並行読みでないとパイプが詰まって固まる
        let r = sb.runner(
            "#!/bin/sh\nhead -c 200000 /dev/zero | tr '\\000' 'e' >&2\necho 'OK|X'\n",
            30_000,
            1,
        );
        let res = run(&r).await;
        assert!(matches!(res, Err(KbError::ProcessFailed(_))), "{res:?}");
    }

    #[tokio::test]
    async fn timeout_kills_and_cleans_up() {
        let sb = Sandbox::new();
        let r = sb.runner("#!/bin/sh\nsleep 5\n", 200, 1);
        let started = std::time::Instant::now();
        let res = run(&r).await;
        assert_eq!(res, Err(KbError::Timeout));
        assert!(started.elapsed() < Duration::from_secs(3));
        assert_eq!(sb.leftover_files(), 0);
    }

    #[tokio::test]
    async fn missing_binary_is_error() {
        let sb = Sandbox::new();
        let r = Runner::new("/nonexistent/scryer-prolog", Duration::from_secs(1), 1)
            .with_tmp_dir(sb.dir.clone());
        let res = run(&r).await;
        assert!(matches!(res, Err(KbError::ProcessFailed(_))), "{res:?}");
        let left = fs::read_dir(&sb.dir).map(|d| d.count()).unwrap_or(0);
        assert_eq!(left, 0, "temp file must be removed even if spawn fails");
    }

    #[tokio::test]
    async fn over_capacity_is_busy_and_permit_is_released() {
        let sb = Sandbox::new();
        let r = Arc::new(sb.runner("#!/bin/sh\nsleep 1\necho 'OK|X'\n", 30_000, 1));
        let r1 = Arc::clone(&r);
        let first = tokio::spawn(async move { run(&r1).await });
        tokio::time::sleep(Duration::from_millis(300)).await;
        // 1つ目が実行中の間は、待たずに Busy
        assert_eq!(run(&r).await, Err(KbError::Busy));
        assert!(first.await.expect("join").is_ok());
        // 終わったら再び実行できる(permit が解放されている)
        assert!(run(&r).await.is_ok());
    }

    #[tokio::test]
    async fn permit_is_released_after_timeout() {
        let sb = Sandbox::new();
        let r = sb.runner("#!/bin/sh\nsleep 5\n", 200, 1);
        assert_eq!(run(&r).await, Err(KbError::Timeout));
        assert_eq!(run(&r).await, Err(KbError::Timeout), "not Busy");
    }
}
