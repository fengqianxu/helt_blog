ALTER TABLE moments
    DROP CONSTRAINT moments_content_not_blank;

CREATE TABLE moment_like_attempts (
    id BIGSERIAL PRIMARY KEY,
    moment_id BIGINT NOT NULL REFERENCES moments(id) ON DELETE CASCADE,
    visitor_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT moment_like_attempts_visitor_not_blank CHECK (btrim(visitor_id) <> '')
);

CREATE INDEX idx_moment_like_attempts_visitor_time
    ON moment_like_attempts (visitor_id, created_at DESC);
