ALTER TABLE public.tickets_comment
    ADD COLUMN IF NOT EXISTS updated_at timestamp with time zone;
