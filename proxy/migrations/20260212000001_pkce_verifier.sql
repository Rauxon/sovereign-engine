-- Add PKCE verifier to OIDC auth state for providers that require it.
ALTER TABLE oidc_auth_state ADD COLUMN pkce_verifier TEXT;
