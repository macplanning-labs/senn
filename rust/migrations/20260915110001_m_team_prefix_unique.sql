-- DEMO-000168: m_team.prefix の一意性を DB で保証（大小無視）
-- NULL / 空文字は対象外（既存の nullable 方針を維持）

DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM (
      SELECT UPPER(prefix) AS up
      FROM m_team
      WHERE prefix IS NOT NULL AND btrim(prefix) <> ''
      GROUP BY UPPER(prefix)
      HAVING COUNT(*) > 1
    ) d
  ) THEN
    RAISE EXCEPTION 'm_team.prefix has duplicate values (case-insensitive); resolve before applying uq_m_team_prefix_upper';
  END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS uq_m_team_prefix_upper
  ON m_team (UPPER(prefix))
  WHERE prefix IS NOT NULL AND btrim(prefix) <> '';
