ALTER TABLE site_settings
    ADD COLUMN steam_web_api_key_ciphertext BYTEA,
    ADD COLUMN steam_web_api_key_nonce BYTEA,
    ADD COLUMN steam_encryption_key_version INTEGER;

ALTER TABLE site_settings
    ADD CONSTRAINT site_settings_steam_key_complete
        CHECK (
            (steam_web_api_key_ciphertext IS NULL
                AND steam_web_api_key_nonce IS NULL
                AND steam_encryption_key_version IS NULL)
            OR
            (steam_web_api_key_ciphertext IS NOT NULL
                AND steam_web_api_key_nonce IS NOT NULL
                AND steam_encryption_key_version IS NOT NULL)
        ),
    ADD CONSTRAINT site_settings_steam_key_nonce_length
        CHECK (
            steam_web_api_key_nonce IS NULL
            OR octet_length(steam_web_api_key_nonce) = 24
        ),
    ADD CONSTRAINT site_settings_steam_key_version_positive
        CHECK (
            steam_encryption_key_version IS NULL
            OR steam_encryption_key_version > 0
        );
