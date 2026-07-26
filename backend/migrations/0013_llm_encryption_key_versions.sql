-- Version every encrypted credential so deployments can keep a current and
-- previous key during rotation. Existing ciphertext predates versioning and is
-- assigned version 1.
ALTER TABLE llm_connections
    ADD COLUMN encryption_key_version INTEGER;

UPDATE llm_connections
SET encryption_key_version = 1
WHERE api_key_ciphertext IS NOT NULL;

ALTER TABLE llm_connections
    DROP CONSTRAINT llm_connections_key_pair,
    ADD CONSTRAINT llm_connections_encrypted_key_complete CHECK (
        (api_key_ciphertext IS NULL AND api_key_nonce IS NULL AND encryption_key_version IS NULL)
        OR (
            api_key_ciphertext IS NOT NULL
            AND api_key_nonce IS NOT NULL
            AND encryption_key_version IS NOT NULL
            AND encryption_key_version > 0
        )
    );

-- 0011 copied the singleton credential into llm_connections. Remove the stale
-- source columns now that all application reads use the collection table.
UPDATE llm_settings
SET api_key_ciphertext = NULL,
    api_key_nonce = NULL
WHERE id = 1;

ALTER TABLE llm_settings
    DROP CONSTRAINT llm_settings_key_pair,
    DROP COLUMN api_key_ciphertext,
    DROP COLUMN api_key_nonce;
