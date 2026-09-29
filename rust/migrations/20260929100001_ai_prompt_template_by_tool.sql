-- AI prompt template (おまじない) by tool
-- tickets_project に ai_prompt_template_cursor, ai_prompt_template_claude 列追加

ALTER TABLE tickets_project
  ADD COLUMN ai_prompt_template_cursor TEXT,
  ADD COLUMN ai_prompt_template_claude TEXT;
