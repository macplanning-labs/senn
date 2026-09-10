-- サイクルのアクティブ化/完了タイムスタンプ(スコープクリープ集計の基準時刻、
-- および自動進行機能で使用)
ALTER TABLE t_cycle ADD COLUMN activated_at TIMESTAMPTZ NULL;
ALTER TABLE t_cycle ADD COLUMN completed_at TIMESTAMPTZ NULL;
