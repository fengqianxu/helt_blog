-- Allow administrators to manage multiple OpenAI-compatible credentials.
-- The singleton keeps the shared use-case prompts and optimistic-lock revision;
-- connection credentials and model defaults live in this separate collection.
CREATE TABLE llm_connections (
    id BIGSERIAL PRIMARY KEY,
    display_name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    model TEXT NOT NULL,
    api_key_ciphertext BYTEA,
    api_key_nonce BYTEA,
    temperature REAL NOT NULL DEFAULT 0.7,
    max_tokens INTEGER NOT NULL DEFAULT 512,
    enabled BOOLEAN NOT NULL DEFAULT true,
    last_tested_at TIMESTAMPTZ,
    last_test_status TEXT NOT NULL DEFAULT 'untested',
    last_test_latency_ms INTEGER,
    last_test_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT llm_connections_name_not_blank CHECK (btrim(display_name) <> ''),
    CONSTRAINT llm_connections_base_url_not_blank CHECK (btrim(base_url) <> ''),
    CONSTRAINT llm_connections_model_not_blank CHECK (btrim(model) <> ''),
    CONSTRAINT llm_connections_key_pair CHECK (
        (api_key_ciphertext IS NULL AND api_key_nonce IS NULL)
        OR (api_key_ciphertext IS NOT NULL AND api_key_nonce IS NOT NULL)
    ),
    CONSTRAINT llm_connections_temperature_valid CHECK (temperature BETWEEN 0 AND 2),
    CONSTRAINT llm_connections_max_tokens_valid CHECK (max_tokens BETWEEN 1 AND 8192),
    CONSTRAINT llm_connections_test_status_valid CHECK (last_test_status IN ('untested', 'online', 'error')),
    CONSTRAINT llm_connections_latency_non_negative CHECK (
        last_test_latency_ms IS NULL OR last_test_latency_ms >= 0
    )
);

CREATE TRIGGER trg_llm_connections_updated_at
BEFORE UPDATE ON llm_connections
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Migrate the old singleton connection when it was complete. Empty legacy
-- defaults intentionally remain empty so the administrator can add a new key
-- from the management page without creating a bogus provider entry.
INSERT INTO llm_connections (
    display_name, base_url, model, api_key_ciphertext, api_key_nonce,
    temperature, max_tokens, enabled, last_tested_at, last_test_status,
    last_test_latency_ms, last_test_error, created_at, updated_at
)
SELECT
    display_name, base_url, model, api_key_ciphertext, api_key_nonce,
    temperature, max_tokens, enabled, last_tested_at, last_test_status,
    last_test_latency_ms, last_test_error, created_at, updated_at
FROM llm_settings
WHERE id = 1
  AND btrim(base_url) <> ''
  AND btrim(model) <> '';
