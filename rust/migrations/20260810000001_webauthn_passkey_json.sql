-- Add passkey_json column to mfa_webauthn_credential for storing complete Passkey struct
-- This enables proper Passkey deserialization for authentication (future use)

ALTER TABLE mfa_webauthn_credential
ADD COLUMN passkey_json TEXT;
