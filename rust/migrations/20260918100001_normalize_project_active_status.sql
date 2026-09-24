-- Existing projects were all created with status='active' before status became
-- a real lifecycle field with 4 canonical values. Normalize them to 'in_progress'.
UPDATE tickets_project SET status = 'in_progress' WHERE status = 'active';
