-- LLM uses one user-supplied OpenAI-compatible API address. The legacy
-- provider discriminator is no longer part of the settings contract.
ALTER TABLE llm_settings
    DROP CONSTRAINT IF EXISTS llm_settings_provider_valid,
    DROP CONSTRAINT IF EXISTS llm_settings_base_url_not_blank,
    DROP CONSTRAINT IF EXISTS llm_settings_model_not_blank,
    DROP COLUMN IF EXISTS provider;

ALTER TABLE llm_settings
    ALTER COLUMN base_url SET DEFAULT '',
    ALTER COLUMN model SET DEFAULT '';

UPDATE llm_settings
SET base_url = ''
WHERE id = 1 AND base_url = 'https://api.openai.com/v1';

UPDATE llm_settings
SET model = ''
WHERE id = 1 AND model = 'gpt-4.1-mini';
