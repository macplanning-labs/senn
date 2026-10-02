-- アクセス制御の再設計 フェーズ F-3(詳細設計書 §10.6)
-- 個人の AI キーで作ったチケットは、作成者を「キーの持ち主本人」にする。
-- AI 経由であることは、この列で残す(作成者の欄に共有の AI アカウントを入れる今のやり方の代わり)。
-- 既存の行は書き換えない(共有の AI アカウントが作成者の行は、表示の側で AI 経由として扱う)。
-- 再実行しても失敗しない形にする(IF NOT EXISTS)。

ALTER TABLE public.tickets_ticket
  ADD COLUMN IF NOT EXISTS created_via_ai boolean NOT NULL DEFAULT false;

ALTER TABLE public.tickets_ticket
  ADD COLUMN IF NOT EXISTS created_via_ai_key_id bigint NULL
    REFERENCES public.ai_agent_api_keys(id) ON DELETE SET NULL;
