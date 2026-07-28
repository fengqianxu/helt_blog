CREATE TABLE artalk_outbox (
    id BIGSERIAL PRIMARY KEY,
    aggregate_key TEXT UNIQUE NOT NULL,
    operation TEXT NOT NULL,
    payload JSONB NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT artalk_outbox_key_not_blank CHECK (btrim(aggregate_key) <> ''),
    CONSTRAINT artalk_outbox_operation_valid
        CHECK (operation IN ('set_commenting', 'delete_page')),
    CONSTRAINT artalk_outbox_attempts_non_negative CHECK (attempts >= 0)
);

CREATE INDEX idx_artalk_outbox_due
    ON artalk_outbox (next_attempt_at, id);
