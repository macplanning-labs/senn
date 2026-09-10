-- ラベルのAI自動分類・100名規模運用対応
-- description(AI判定ルール), category(type/severity/cause/scope), is_ai_enabled を追加

ALTER TABLE m_label ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE m_label ADD COLUMN IF NOT EXISTS category VARCHAR(50);
ALTER TABLE m_label ADD COLUMN IF NOT EXISTS is_ai_enabled BOOLEAN NOT NULL DEFAULT true;

-- 標準ラベルを全プロジェクトへ初期登録(既存分は重複登録しない)
INSERT INTO m_label (name, color, category, description, is_ai_enabled, created_at, project_id)
SELECT v.name, v.color, v.category, v.description, true, NOW(), p.id
FROM tickets_project p
CROSS JOIN (VALUES
    ('type:incident', '#E53E3E', 'type', '本番環境で発生した緊急障害。サービス停止やデータ損壊を伴う場合。'),
    ('type:bug', '#DD6B20', 'type', '開発中・QA中または特定環境で発生した不具合・仕様との差異。'),
    ('type:feature', '#3182CE', 'type', '新機能追加・仕様拡張。'),
    ('type:improvement', '#38A169', 'type', '既存機能の改善・リファクタリング・速度改善。'),
    ('type:task', '#805AD5', 'type', '開発以外の作業・調査・環境構築・ドキュメント作成。'),
    ('severity:critical', '#E53E3E', 'severity', '最重要。システム停止や業務不可能な問題。'),
    ('severity:major', '#DD6B20', 'severity', '重大。主要機能に支障があるが回避策が存在する問題。'),
    ('severity:minor', '#D69E2E', 'severity', '軽微。一部の表示崩れや機能不具合。'),
    ('cause:spec-gap', '#718096', 'cause', '仕様の考慮漏れ・要件の不備による不具合。'),
    ('cause:regression', '#E53E3E', 'cause', 'デグレ・先祖返りによる不具合。')
) AS v(name, color, category, description)
WHERE NOT EXISTS (
    SELECT 1 FROM m_label existing
    WHERE existing.project_id = p.id AND existing.name = v.name
);
