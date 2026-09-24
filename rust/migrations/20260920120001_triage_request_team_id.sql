-- DEMO-000195: トリアージをチーム単位の受信箱にするため team_id を追加する
ALTER TABLE t_triage_request
  ADD COLUMN IF NOT EXISTS team_id bigint NULL;

DO $$ BEGIN
  ALTER TABLE t_triage_request
    ADD CONSTRAINT t_triage_request_team_id_fk_m_team
    FOREIGN KEY (team_id) REFERENCES public.m_team(id) DEFERRABLE INITIALLY DEFERRED;
EXCEPTION
  WHEN duplicate_object THEN NULL;
END $$;

CREATE INDEX IF NOT EXISTS t_triage_request_team_id_idx ON t_triage_request (team_id);

-- 既存行: 起票先プロジェクトに参加している先頭チームへ寄せる（無い場合は NULL のまま）
UPDATE t_triage_request tr
SET team_id = (
  SELECT pt.team_id
  FROM tickets_project_teams pt
  WHERE pt.project_id = tr.project_id
  ORDER BY pt.team_id
  LIMIT 1
)
WHERE tr.team_id IS NULL
  AND tr.project_id IS NOT NULL;
