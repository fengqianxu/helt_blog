ALTER TABLE admin_users
    ADD COLUMN session_version BIGINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT admin_users_session_version_positive CHECK (session_version > 0);

CREATE TABLE auth_sessions (
    id UUID PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    session_version BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT auth_sessions_version_positive CHECK (session_version > 0)
);

CREATE INDEX idx_auth_sessions_user_active
    ON auth_sessions (user_id, expires_at)
    WHERE revoked_at IS NULL;
CREATE INDEX idx_auth_sessions_expiry ON auth_sessions (expires_at);

-- Existing refresh tokens cannot be bound to the access token that created
-- them. Expire these ephemeral sessions once during the security upgrade.
TRUNCATE TABLE refresh_tokens;
ALTER TABLE refresh_tokens
    ADD COLUMN session_id UUID NOT NULL REFERENCES auth_sessions(id) ON DELETE CASCADE;
CREATE INDEX idx_refresh_tokens_session ON refresh_tokens (session_id);
