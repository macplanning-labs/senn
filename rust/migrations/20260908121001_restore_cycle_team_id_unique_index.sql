-- 20260908000501でt_cycle.team_idを一時的に外した際、それに依存していた
-- partial unique index(unique_cycle_number_per_team_no_project、20260908000001で作成)も
-- 連鎖して自動削除される。20260908110001でteam_idが再作成・バックフィルされた後、
-- このインデックスを復元する。
--
-- 列が削除されなかった環境(staging等、20260908000501がno-opだった場合)では
-- インデックスはそのまま残っているため、本ファイルはno-op。
CREATE UNIQUE INDEX IF NOT EXISTS unique_cycle_number_per_team_no_project
    ON t_cycle (team_id, number)
    WHERE project_id IS NULL AND team_id IS NOT NULL;
