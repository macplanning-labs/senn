-- 「resolved」ワークフローステータスの表示名を "Done" から "Resolved" に統一する。
--
-- 背景: resource_repo.rs::create_project / team_repo.rs::create_team のデフォルト投入で
-- slug='resolved' の表示名が "Done" になっていたが、一覧・カンバン等のフロント側は
-- ハードコードされた "Resolved" ラベルを使っており、詳細パネルだけ DB の name をそのまま
-- 表示していたため、同じチケットが画面によって「Resolved」「Done」と食い違って見えていた。
-- ユーザーが自分でリネームした行(name が "Done" 以外)は対象外。

UPDATE t_workflow_status
SET name = 'Resolved'
WHERE slug = 'resolved' AND name = 'Done';
