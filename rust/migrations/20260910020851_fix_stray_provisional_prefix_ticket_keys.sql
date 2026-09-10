-- Data fix: ticket_key is generated once at INSERT time from the ticket's team.prefix
-- (ticket_repo::api_generate_ticket_key) and never recomputed. A handful of tickets were
-- created while their team still carried an auto-derived provisional prefix (from
-- 20260908000001_g6_1_nullable_project_id.sql's slug-based backfill, e.g. slug
-- "prov-11-prs" -> "PROV11PRS") or an owner team's own prefix (team 7 "Wipアプリ開発チーム",
-- prefix "WIPAPPDEV"), before the project's real prefix (tickets_ticket.project_id ->
-- tickets_project.prefix) was established as the one actually used going forward. This
-- backfills those stray keys to match the project's real prefix, continuing that
-- project's existing numbering.
--
-- Renumbering is computed live (MAX + 1 against the real prefix) rather than hardcoded,
-- so this is safe to write once and not dependent on the exact row count at authoring time.

DO $$
DECLARE
  v_next INTEGER;
  v_row RECORD;
BEGIN
  -- PRS (project 11): PROV11PRS-000001 -> PRS-<next>
  IF EXISTS (
    SELECT 1 FROM tickets_ticket
    WHERE ticket_key = 'PROV11PRS-000001' AND project_id = 11
  ) THEN
    SELECT COALESCE(MAX(SUBSTRING(ticket_key FROM 'PRS-(\d+)$')::INTEGER), 0) + 1
      INTO v_next
      FROM tickets_ticket
      WHERE project_id = 11 AND ticket_key ~ '^PRS-\d+$';

    UPDATE tickets_ticket
      SET ticket_key = 'PRS-' || LPAD(v_next::text, 6, '0'), updated_at = NOW()
      WHERE ticket_key = 'PROV11PRS-000001' AND project_id = 11;
  END IF;

  -- OPS (project 6): PROV6OPS-000001 -> OPS-<next>
  IF EXISTS (
    SELECT 1 FROM tickets_ticket
    WHERE ticket_key = 'PROV6OPS-000001' AND project_id = 6
  ) THEN
    SELECT COALESCE(MAX(SUBSTRING(ticket_key FROM 'OPS-(\d+)$')::INTEGER), 0) + 1
      INTO v_next
      FROM tickets_ticket
      WHERE project_id = 6 AND ticket_key ~ '^OPS-\d+$';

    UPDATE tickets_ticket
      SET ticket_key = 'OPS-' || LPAD(v_next::text, 6, '0'), updated_at = NOW()
      WHERE ticket_key = 'PROV6OPS-000001' AND project_id = 6;
  END IF;

  -- FTFC (project 16): WIPAPPDEV-000001..000012 -> FTFC-<next>.. in ascending
  -- WIPAPPDEV numeric order, preserving original creation order.
  FOR v_row IN
    SELECT ticket_key
      FROM tickets_ticket
      WHERE project_id = 16 AND ticket_key ~ '^WIPAPPDEV-\d+$'
      ORDER BY SUBSTRING(ticket_key FROM 'WIPAPPDEV-(\d+)$')::INTEGER
  LOOP
    SELECT COALESCE(MAX(SUBSTRING(ticket_key FROM 'FTFC-(\d+)$')::INTEGER), 0) + 1
      INTO v_next
      FROM tickets_ticket
      WHERE project_id = 16 AND ticket_key ~ '^FTFC-\d+$';

    UPDATE tickets_ticket
      SET ticket_key = 'FTFC-' || LPAD(v_next::text, 6, '0'), updated_at = NOW()
      WHERE ticket_key = v_row.ticket_key AND project_id = 16;
  END LOOP;
END $$;
