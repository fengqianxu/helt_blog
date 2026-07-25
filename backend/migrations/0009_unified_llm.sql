-- Central AI integration. Business modules reference a use-case key from this
-- singleton instead of storing provider credentials or model parameters.
CREATE TABLE llm_settings (
    id SMALLINT PRIMARY KEY DEFAULT 1,
    display_name TEXT NOT NULL DEFAULT '主模型连接',
    provider TEXT NOT NULL DEFAULT 'openai_compatible',
    base_url TEXT NOT NULL DEFAULT 'https://api.openai.com/v1',
    model TEXT NOT NULL DEFAULT 'gpt-4.1-mini',
    api_key_ciphertext BYTEA,
    api_key_nonce BYTEA,
    temperature REAL NOT NULL DEFAULT 0.7,
    max_tokens INTEGER NOT NULL DEFAULT 512,
    enabled BOOLEAN NOT NULL DEFAULT false,
    use_cases JSONB NOT NULL DEFAULT '{}'::jsonb,
    revision BIGINT NOT NULL DEFAULT 1,
    last_tested_at TIMESTAMPTZ,
    last_test_status TEXT NOT NULL DEFAULT 'untested',
    last_test_latency_ms INTEGER,
    last_test_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT llm_settings_singleton CHECK (id = 1),
    CONSTRAINT llm_settings_name_not_blank CHECK (btrim(display_name) <> ''),
    CONSTRAINT llm_settings_provider_valid CHECK (provider IN ('openai_compatible', 'anthropic')),
    CONSTRAINT llm_settings_base_url_not_blank CHECK (btrim(base_url) <> ''),
    CONSTRAINT llm_settings_model_not_blank CHECK (btrim(model) <> ''),
    CONSTRAINT llm_settings_key_pair CHECK (
        (api_key_ciphertext IS NULL AND api_key_nonce IS NULL)
        OR (api_key_ciphertext IS NOT NULL AND api_key_nonce IS NOT NULL)
    ),
    CONSTRAINT llm_settings_temperature_valid CHECK (temperature BETWEEN 0 AND 2),
    CONSTRAINT llm_settings_max_tokens_valid CHECK (max_tokens BETWEEN 1 AND 8192),
    CONSTRAINT llm_settings_use_cases_object CHECK (jsonb_typeof(use_cases) = 'object'),
    CONSTRAINT llm_settings_revision_positive CHECK (revision > 0),
    CONSTRAINT llm_settings_test_status_valid CHECK (last_test_status IN ('untested', 'online', 'error')),
    CONSTRAINT llm_settings_latency_non_negative CHECK (last_test_latency_ms IS NULL OR last_test_latency_ms >= 0)
);

CREATE TRIGGER trg_llm_settings_updated_at
BEFORE UPDATE ON llm_settings
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

INSERT INTO llm_settings (id, use_cases)
SELECT
    1,
    jsonb_build_object(
        'kanban_chat', jsonb_build_object(
            'enabled', true,
            'system_prompt', '你是 helt. 博客的看板娘。结合当前页面语境简短、友好地回答访客，每次回复不超过三句话。'
        ),
        'comment_review', jsonb_build_object(
            'enabled', COALESCE((settings #>> '{comments,ai_review_enabled}')::boolean, false),
            'system_prompt', '判断评论是否为垃圾内容，只返回 normal 或 suspected_spam，并给出 0 到 1 的置信度。'
        ),
        'article_assistant', jsonb_build_object(
            'enabled', false,
            'system_prompt', '协助管理员润色文章，但不得虚构事实或改变原意。'
        )
    )
FROM site_settings
WHERE id = 1
ON CONFLICT (id) DO NOTHING;

-- Remove legacy provider/model ownership. Existing display/persona data remains
-- in the raiment domain, while prompts and provider settings live above.
UPDATE site_settings
SET settings = settings #- '{comments,ai_review_enabled}'
WHERE id = 1;

UPDATE kanban_config
SET config = config - 'model' - 'temperature' - 'max_tokens' - 'read_article_context' - 'prompts'
WHERE id = 1;
