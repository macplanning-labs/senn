-- DEMO-000166: t_cycle / tickets_ticket の team_id FK を ON DELETE RESTRICT に変更
-- NOT NULL（cycle）および project 付き ticket の trigger と SET NULL が衝突して 500 になるため

ALTER TABLE t_cycle DROP CONSTRAINT IF EXISTS t_cycle_team_id_fkey;
ALTER TABLE t_cycle
  ADD CONSTRAINT t_cycle_team_id_fkey
  FOREIGN KEY (team_id) REFERENCES m_team(id) ON DELETE RESTRICT;

ALTER TABLE tickets_ticket DROP CONSTRAINT IF EXISTS tickets_ticket_team_id_fkey;
ALTER TABLE tickets_ticket
  ADD CONSTRAINT tickets_ticket_team_id_fkey
  FOREIGN KEY (team_id) REFERENCES m_team(id) ON DELETE RESTRICT;
