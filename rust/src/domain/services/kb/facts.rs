//! Prolog 事実ビルダー

use super::KbError;

const MAX_FACTS_SIZE: usize = 1_000_000; // 1MB

/// 事実を生成・積み上げるビルダー
#[derive(Debug, Clone)]
pub struct Facts {
    buffer: String,
    size: usize,
    count: usize,
}

impl Facts {
    /// 新しい Facts ビルダーを作成
    pub fn new() -> Self {
        Facts {
            buffer: String::new(),
            size: 0,
            count: 0,
        }
    }

    /// 述語を追加
    ///
    /// # 入力検証
    /// - `name`: `^[a-z][a-z0-9_]*$` のみ許可(小文字で始まる、英数字とアンダースコア)
    /// - `args`: Prolog アトム(クォート済み)
    ///
    /// 違反は `Err(InvalidInput)` を返す。
    /// 成功時は `name(arg1, arg2, ...).` の形式で1行追加。
    pub fn fact(&mut self, name: &str, args: &[String]) -> Result<(), KbError> {
        // 述語名の検証
        if name.is_empty() {
            return Err(KbError::InvalidInput("predicate name is empty".to_string()));
        }

        if !name.chars().next().is_some_and(|c| c.is_ascii_lowercase()) {
            return Err(KbError::InvalidInput(format!(
                "predicate name must start with lowercase: {}",
                name
            )));
        }

        if !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(KbError::InvalidInput(format!(
                "predicate name contains invalid characters: {}",
                name
            )));
        }

        // 1行を生成
        let mut line = String::new();
        line.push_str(name);
        line.push('(');

        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                line.push(',');
            }
            line.push_str(arg);
        }

        line.push_str(").\n");

        // サイズチェック
        let new_size = self.size + line.len();
        if new_size > MAX_FACTS_SIZE {
            return Err(KbError::TooLarge);
        }

        self.buffer.push_str(&line);
        self.size = new_size;
        self.count += 1;

        Ok(())
    }

    /// 生成された事実を文字列として取得
    pub fn as_str(&self) -> &str {
        &self.buffer
    }

    /// 生成された事実の行数
    pub fn len(&self) -> usize {
        self.count
    }

    /// 末尾に整合性用の `facts_count(N).` を付けた文字列を返す。
    /// N はこれまでに追加した事実の行数(facts_count 自身は含めない)。
    /// Prolog 側は読み込めた件数と照合し、構文エラーで黙って捨てられた事実を検出する。
    pub fn finish(&self) -> String {
        format!("{}facts_count({}).\n", self.buffer, self.count)
    }

    /// 事実が空か
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

impl Default for Facts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_facts_basic() {
        let mut facts = Facts::new();
        assert!(facts.fact("target", &["'KEY-001'".to_string()]).is_ok());
        assert!(facts.as_str().contains("target('KEY-001')."));
    }

    #[test]
    fn test_facts_multiple_args() {
        let mut facts = Facts::new();
        facts
            .fact(
                "ticket",
                &[
                    "'KEY'".to_string(),
                    "'bug'".to_string(),
                    "'open'".to_string(),
                    "'none'".to_string(),
                ],
            )
            .unwrap();
        assert!(facts
            .as_str()
            .contains("ticket('KEY','bug','open','none')."));
    }

    #[test]
    fn test_facts_invalid_name_starts_uppercase() {
        let mut facts = Facts::new();
        let result = facts.fact("Target", &[]);
        assert!(matches!(result, Err(KbError::InvalidInput(_))));
    }

    #[test]
    fn test_facts_invalid_name_contains_dash() {
        let mut facts = Facts::new();
        let result = facts.fact("ticket-name", &[]);
        assert!(matches!(result, Err(KbError::InvalidInput(_))));
    }

    #[test]
    fn test_facts_invalid_name_empty() {
        let mut facts = Facts::new();
        let result = facts.fact("", &[]);
        assert!(matches!(result, Err(KbError::InvalidInput(_))));
    }

    #[test]
    fn test_facts_valid_underscore() {
        let mut facts = Facts::new();
        assert!(facts.fact("my_predicate", &["'value'".to_string()]).is_ok());
    }

    #[test]
    fn test_facts_size_limit() {
        let mut facts = Facts::new();
        // Create an argument that, when formatted as a fact, exceeds 1MB
        // The formatted fact will be: large('...').\n
        // So we need an arg that's large enough to push total over 1MB
        let large_arg = "a".repeat(1_000_100);
        let result = facts.fact("large", &[format!("'{}'", large_arg)]);
        assert!(matches!(result, Err(KbError::TooLarge)));
    }

    #[test]
    fn test_facts_accumulation() {
        let mut facts = Facts::new();
        facts.fact("a", &["'1'".to_string()]).unwrap();
        facts.fact("b", &["'2'".to_string()]).unwrap();
        facts.fact("c", &["'3'".to_string()]).unwrap();

        let s = facts.as_str();
        assert!(s.contains("a('1')."));
        assert!(s.contains("b('2')."));
        assert!(s.contains("c('3')."));
        assert_eq!(facts.len(), 3);
    }

    #[test]
    fn test_facts_empty() {
        let facts = Facts::new();
        assert!(facts.is_empty());
        assert_eq!(facts.len(), 0);
    }

    #[test]
    fn test_facts_len() {
        let mut facts = Facts::new();
        facts.fact("a", &[]).unwrap();
        facts.fact("b", &[]).unwrap();
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn finish_appends_fact_count_without_counting_itself() {
        let mut facts = Facts::new();
        facts.fact("a", &["'1'".to_string()]).expect("ok");
        facts
            .fact("b", &["'2'".to_string(), "3".to_string()])
            .expect("ok");
        assert_eq!(facts.len(), 2);
        assert_eq!(facts.finish(), "a('1').\nb('2',3).\nfacts_count(2).\n");
        // finish() は元の状態を変えない
        assert_eq!(facts.len(), 2);
    }

    #[test]
    fn finish_on_empty_reports_zero() {
        assert_eq!(Facts::new().finish(), "facts_count(0).\n");
    }
}
