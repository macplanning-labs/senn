-- AI prompt template (おまじない) + cache
-- tickets_project に ai_prompt_template 列追加
-- tickets_ticket に ai_prompt, ai_prompt_updated_at, ai_prompt_generation_mode 列追加

ALTER TABLE tickets_project
  ADD COLUMN ai_prompt_template TEXT;

ALTER TABLE tickets_ticket
  ADD COLUMN ai_prompt TEXT,
  ADD COLUMN ai_prompt_updated_at TIMESTAMP WITH TIME ZONE,
  ADD COLUMN ai_prompt_generation_mode VARCHAR(50);
