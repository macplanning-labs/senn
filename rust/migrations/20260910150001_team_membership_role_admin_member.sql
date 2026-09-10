-- Team 薄い Role を admin / member のみに揃える（leader → admin）
-- 細粒度 ACL は作らない

UPDATE public.t_team_membership
SET role = 'admin'
WHERE role = 'leader';

-- 想定外の値が残っていれば member に寄せる（壊さない）
UPDATE public.t_team_membership
SET role = 'member'
WHERE role NOT IN ('admin', 'member');

ALTER TABLE public.t_team_membership
DROP CONSTRAINT IF EXISTS check_team_membership_role;

ALTER TABLE public.t_team_membership
ADD CONSTRAINT check_team_membership_role
CHECK (role IN ('admin', 'member'));
