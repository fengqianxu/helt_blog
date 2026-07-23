CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE categories (
    id BIGSERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    color TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT categories_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT categories_slug_not_blank CHECK (btrim(slug) <> ''),
    CONSTRAINT categories_color_format CHECK (color = '' OR color ~ '^#[0-9A-Fa-f]{6}$')
);

CREATE TABLE articles (
    id BIGSERIAL PRIMARY KEY,
    slug TEXT UNIQUE NOT NULL,
    title TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    content_md TEXT NOT NULL DEFAULT '',
    cover_key TEXT,
    category_id BIGINT REFERENCES categories(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    is_pinned BOOLEAN NOT NULL DEFAULT FALSE,
    allow_comment BOOLEAN NOT NULL DEFAULT TRUE,
    kanban_ref BOOLEAN NOT NULL DEFAULT TRUE,
    word_count INTEGER NOT NULL DEFAULT 0,
    read_minutes INTEGER NOT NULL DEFAULT 0,
    view_count BIGINT NOT NULL DEFAULT 0,
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT articles_slug_not_blank CHECK (btrim(slug) <> ''),
    CONSTRAINT articles_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT articles_status_valid CHECK (status IN ('draft', 'published', 'hidden')),
    CONSTRAINT articles_counts_non_negative CHECK (word_count >= 0 AND read_minutes >= 0 AND view_count >= 0),
    CONSTRAINT articles_publish_time_consistent CHECK (status <> 'published' OR published_at IS NOT NULL)
);

CREATE INDEX idx_articles_list ON articles (status, is_pinned DESC, published_at DESC);
CREATE INDEX idx_articles_category ON articles (category_id, status, published_at DESC);
CREATE INDEX idx_articles_trgm ON articles USING GIN (title gin_trgm_ops, content_md gin_trgm_ops);

CREATE TRIGGER trg_articles_updated_at
BEFORE UPDATE ON articles
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE tags (
    id BIGSERIAL PRIMARY KEY,
    name TEXT UNIQUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT tags_name_not_blank CHECK (btrim(name) <> '')
);

CREATE TABLE article_tags (
    article_id BIGINT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    tag_id BIGINT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (article_id, tag_id)
);

CREATE INDEX idx_article_tags_tag ON article_tags (tag_id, article_id);

CREATE TABLE comments (
    id BIGSERIAL PRIMARY KEY,
    target_type TEXT NOT NULL,
    target_id BIGINT NOT NULL,
    parent_id BIGINT REFERENCES comments(id) ON DELETE CASCADE,
    author_name TEXT NOT NULL,
    author_email TEXT NOT NULL DEFAULT '',
    author_site TEXT NOT NULL DEFAULT '',
    is_owner BOOLEAN NOT NULL DEFAULT FALSE,
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    ai_verdict TEXT,
    ai_confidence REAL,
    ip_hash TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT comments_target_type_valid CHECK (target_type IN ('article', 'moment')),
    CONSTRAINT comments_target_id_positive CHECK (target_id > 0),
    CONSTRAINT comments_author_not_blank CHECK (btrim(author_name) <> ''),
    CONSTRAINT comments_content_not_blank CHECK (btrim(content) <> ''),
    CONSTRAINT comments_status_valid CHECK (status IN ('pending', 'approved', 'spam')),
    CONSTRAINT comments_ai_verdict_valid CHECK (ai_verdict IS NULL OR ai_verdict IN ('normal', 'suspected_spam')),
    CONSTRAINT comments_ai_confidence_valid CHECK (ai_confidence IS NULL OR ai_confidence BETWEEN 0 AND 1),
    CONSTRAINT comments_not_self_parent CHECK (parent_id IS NULL OR parent_id <> id)
);

CREATE INDEX idx_comments_target ON comments (target_type, target_id, status, created_at);
CREATE INDEX idx_comments_status ON comments (status, created_at DESC);
CREATE INDEX idx_comments_parent ON comments (parent_id) WHERE parent_id IS NOT NULL;

CREATE TABLE moments (
    id BIGSERIAL PRIMARY KEY,
    content TEXT NOT NULL,
    images JSONB NOT NULL DEFAULT '[]'::jsonb,
    like_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT moments_content_not_blank CHECK (btrim(content) <> ''),
    CONSTRAINT moments_images_array CHECK (jsonb_typeof(images) = 'array'),
    CONSTRAINT moments_like_count_non_negative CHECK (like_count >= 0)
);

CREATE TABLE moment_likes (
    moment_id BIGINT NOT NULL REFERENCES moments(id) ON DELETE CASCADE,
    visitor_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (moment_id, visitor_id),
    CONSTRAINT moment_likes_visitor_not_blank CHECK (btrim(visitor_id) <> '')
);

CREATE TABLE bangumi (
    id BIGSERIAL PRIMARY KEY,
    bilibili_media_id BIGINT UNIQUE NOT NULL,
    title TEXT NOT NULL,
    cover_key TEXT,
    status TEXT NOT NULL DEFAULT 'watching',
    ep_current INTEGER NOT NULL DEFAULT 0,
    ep_total INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT bangumi_media_id_positive CHECK (bilibili_media_id > 0),
    CONSTRAINT bangumi_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT bangumi_status_valid CHECK (status IN ('watching', 'finished')),
    CONSTRAINT bangumi_episodes_non_negative CHECK (ep_current >= 0 AND ep_total >= 0)
);

CREATE INDEX idx_bangumi_list ON bangumi (status, sort_order, id);

CREATE TABLE games (
    id BIGSERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    cover_key TEXT,
    status TEXT NOT NULL DEFAULT 'playing',
    play_hours REAL NOT NULL DEFAULT 0,
    comment TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT games_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT games_status_valid CHECK (status IN ('playing', 'finished', 'shelved')),
    CONSTRAINT games_hours_non_negative CHECK (play_hours >= 0)
);

CREATE INDEX idx_games_list ON games (status, sort_order, id);

CREATE TABLE friends (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    url TEXT NOT NULL,
    avatar_url TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pending',
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT friends_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT friends_url_not_blank CHECK (btrim(url) <> ''),
    CONSTRAINT friends_status_valid CHECK (status IN ('pending', 'approved', 'rejected'))
);

CREATE INDEX idx_friends_list ON friends (status, sort_order, created_at);

CREATE TABLE site_settings (
    id SMALLINT PRIMARY KEY DEFAULT 1,
    settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT site_settings_singleton CHECK (id = 1),
    CONSTRAINT site_settings_object CHECK (jsonb_typeof(settings) = 'object')
);

CREATE TRIGGER trg_site_settings_updated_at
BEFORE UPDATE ON site_settings
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE theme_configs (
    mode TEXT PRIMARY KEY,
    cover_key TEXT,
    voice_key TEXT,
    quote_jp TEXT NOT NULL DEFAULT '',
    quote_zh TEXT NOT NULL DEFAULT '',
    voice_credit TEXT NOT NULL DEFAULT '',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT theme_configs_mode_valid CHECK (mode IN ('day', 'night'))
);

CREATE TRIGGER trg_theme_configs_updated_at
BEFORE UPDATE ON theme_configs
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE music_tracks (
    id BIGSERIAL PRIMARY KEY,
    title TEXT NOT NULL,
    artist TEXT NOT NULL DEFAULT '',
    file_key TEXT NOT NULL,
    duration_s INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT music_tracks_title_not_blank CHECK (btrim(title) <> ''),
    CONSTRAINT music_tracks_file_not_blank CHECK (btrim(file_key) <> ''),
    CONSTRAINT music_tracks_duration_non_negative CHECK (duration_s >= 0)
);

CREATE INDEX idx_music_tracks_order ON music_tracks (sort_order, id);

CREATE TABLE live2d_models (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    model_key TEXT UNIQUE NOT NULL,
    thumbnail_key TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT live2d_models_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT live2d_models_key_not_blank CHECK (btrim(model_key) <> '')
);

CREATE TABLE kanban_config (
    id SMALLINT PRIMARY KEY DEFAULT 1,
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT kanban_config_singleton CHECK (id = 1),
    CONSTRAINT kanban_config_object CHECK (jsonb_typeof(config) = 'object')
);

CREATE TRIGGER trg_kanban_config_updated_at
BEFORE UPDATE ON kanban_config
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE kanban_chats (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    article_id BIGINT REFERENCES articles(id) ON DELETE SET NULL,
    latency_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT kanban_chats_session_not_blank CHECK (btrim(session_id) <> ''),
    CONSTRAINT kanban_chats_role_valid CHECK (role IN ('user', 'assistant', 'fallback')),
    CONSTRAINT kanban_chats_latency_non_negative CHECK (latency_ms IS NULL OR latency_ms >= 0)
);

CREATE INDEX idx_kanban_chats_created ON kanban_chats (created_at DESC);
CREATE INDEX idx_kanban_chats_session ON kanban_chats (session_id, created_at);

CREATE TABLE admin_users (
    id BIGSERIAL PRIMARY KEY,
    username TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    totp_secret TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT admin_users_username_not_blank CHECK (btrim(username) <> ''),
    CONSTRAINT admin_users_password_hash_not_blank CHECK (btrim(password_hash) <> '')
);

CREATE TABLE passkeys (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    credential_id BYTEA UNIQUE NOT NULL,
    public_key BYTEA NOT NULL,
    sign_count BIGINT NOT NULL DEFAULT 0,
    label TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT passkeys_sign_count_non_negative CHECK (sign_count >= 0)
);

CREATE INDEX idx_passkeys_user ON passkeys (user_id);

CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
    token_hash TEXT UNIQUE NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT refresh_tokens_hash_not_blank CHECK (btrim(token_hash) <> '')
);

CREATE INDEX idx_refresh_tokens_user ON refresh_tokens (user_id);
CREATE INDEX idx_refresh_tokens_expiry ON refresh_tokens (expires_at);

CREATE TABLE daily_stats (
    day DATE PRIMARY KEY,
    pv BIGINT NOT NULL DEFAULT 0,
    uv BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT daily_stats_non_negative CHECK (pv >= 0 AND uv >= 0 AND uv <= pv)
);

CREATE TABLE daily_visitors (
    day DATE NOT NULL,
    visitor_id TEXT NOT NULL,
    PRIMARY KEY (day, visitor_id),
    CONSTRAINT daily_visitors_id_not_blank CHECK (btrim(visitor_id) <> '')
);

CREATE TABLE backups (
    id BIGSERIAL PRIMARY KEY,
    object_key TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    status TEXT NOT NULL DEFAULT 'ok',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT backups_key_not_blank CHECK (btrim(object_key) <> ''),
    CONSTRAINT backups_size_non_negative CHECK (size_bytes >= 0),
    CONSTRAINT backups_status_valid CHECK (status IN ('ok', 'failed'))
);

CREATE INDEX idx_backups_created ON backups (created_at DESC);

CREATE TABLE uploads (
    id BIGSERIAL PRIMARY KEY,
    object_key TEXT UNIQUE NOT NULL,
    bucket TEXT NOT NULL,
    mime TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    kind TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uploads_key_not_blank CHECK (btrim(object_key) <> ''),
    CONSTRAINT uploads_bucket_not_blank CHECK (btrim(bucket) <> ''),
    CONSTRAINT uploads_mime_not_blank CHECK (btrim(mime) <> ''),
    CONSTRAINT uploads_size_non_negative CHECK (size_bytes >= 0),
    CONSTRAINT uploads_kind_valid CHECK (kind IN ('cover', 'article_image', 'bgm', 'voice', 'avatar', 'moment_image'))
);

CREATE INDEX idx_uploads_kind_created ON uploads (kind, created_at DESC);

INSERT INTO categories (name, slug, color, sort_order) VALUES
    ('技术', 'tech', '#45E6FF', 10),
    ('折腾', 'tinkering', '#A678FF', 20),
    ('生活', 'life', '#FF8DBA', 30),
    ('杂谈', 'thoughts', '#FFC857', 40)
ON CONFLICT (slug) DO NOTHING;

INSERT INTO site_settings (id, settings) VALUES (
    1,
    '{
      "basic": {"name": "helt.", "tagline": "记录技术、生活与热爱", "domain": "http://localhost", "icp": "", "founded_at": "2026-07-23"},
      "features": {"comments": true, "kanban": true, "music": true, "stats": true, "easter_egg": true, "rss": true},
      "theme": {"default_mode": "day", "follow_system": true, "schedule": {"day_at": "07:00", "night_at": "19:00"}, "transition_enabled": true, "splash_enabled": true},
      "backup": {"enabled": true, "keep_days": 7},
      "bangumi_sync": {"uid": "", "interval_hours": 6, "last_sync_at": null, "last_status": null, "last_counts": {"watching": 0, "finished": 0}},
      "about": {"avatar_key": null, "bio": "", "intro_md": "", "socials": [], "skills": [], "secret_note": ""},
      "music": {"autoplay": false, "default_volume": 0.5},
      "comments": {"ai_review_enabled": true}
    }'::jsonb
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO theme_configs (mode) VALUES ('day'), ('night')
ON CONFLICT (mode) DO NOTHING;

INSERT INTO kanban_config (id, config) VALUES (
    1,
    '{
      "model": "claude-sonnet-4-5",
      "temperature": 0.7,
      "max_tokens": 256,
      "read_article_context": true,
      "tts_enabled": true,
      "persona_switch": true,
      "prompts": {"day": "", "night": ""},
      "personas": {
        "day": {"name": "Saber", "greeting_template": "欢迎来到 helt."},
        "night": {"name": "Saber Alter", "greeting_template": "夜深了，欢迎回来。"}
      },
      "trigger_words": [],
      "live2d": {"day_model_id": null, "night_model_id": null}
    }'::jsonb
)
ON CONFLICT (id) DO NOTHING;

