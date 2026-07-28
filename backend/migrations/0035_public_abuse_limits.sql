CREATE TABLE site_visit_attempts (
    id BIGSERIAL PRIMARY KEY,
    visitor_fingerprint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT site_visit_attempts_fingerprint_not_blank
        CHECK (btrim(visitor_fingerprint) <> '')
);

CREATE INDEX idx_site_visit_attempts_fingerprint_time
    ON site_visit_attempts (visitor_fingerprint, created_at DESC);

CREATE INDEX idx_site_visit_attempts_created_at
    ON site_visit_attempts (created_at);

-- 0032 predates global retention for this high-churn rate-limit ledger.
CREATE INDEX idx_moment_like_attempts_created_at
    ON moment_like_attempts (created_at);
