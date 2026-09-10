ALTER TABLE public.tickets_comment
    ADD COLUMN IF NOT EXISTS parent_comment_id bigint NULL
        REFERENCES public.tickets_comment(id) ON DELETE CASCADE,
    ADD COLUMN IF NOT EXISTS deleted_at timestamp with time zone NULL;

CREATE INDEX IF NOT EXISTS idx_tickets_comment_parent_comment_id
    ON public.tickets_comment (parent_comment_id);

COMMENT ON COLUMN public.tickets_comment.parent_comment_id IS
    '返信元のトップレベルコメントID。ネストは1階層のみ。NULL=トップレベルコメント。';
COMMENT ON COLUMN public.tickets_comment.deleted_at IS
    '論理削除日時。NULL以外はスレッド構造保持のため一覧には残るが、APIレスポンスのbodyはプレースホルダーに置換される。';
