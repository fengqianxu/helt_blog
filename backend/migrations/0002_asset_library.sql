-- Unified MinIO-backed asset library.
-- `uploads` remains the immutable physical-object ledger. `assets` is the stable
-- logical identity referenced by business records, and `asset_versions` maps a
-- logical asset to every historical MinIO object.

ALTER TABLE uploads DROP CONSTRAINT uploads_kind_valid;
ALTER TABLE uploads
    ADD CONSTRAINT uploads_kind_valid CHECK (
        kind IN (
            'cover',
            'article_image',
            'bgm',
            'voice',
            'avatar',
            'moment_image',
            'image',
            'audio',
            'video',
            'live2d',
            'font',
            'other'
        )
    );
ALTER TABLE uploads
    ADD COLUMN original_filename TEXT,
    ADD COLUMN checksum_sha256 TEXT,
    ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    ADD CONSTRAINT uploads_original_filename_not_blank
        CHECK (original_filename IS NULL OR btrim(original_filename) <> ''),
    ADD CONSTRAINT uploads_checksum_sha256_format
        CHECK (checksum_sha256 IS NULL OR checksum_sha256 ~ '^[0-9A-Fa-f]{64}$'),
    ADD CONSTRAINT uploads_metadata_object
        CHECK (jsonb_typeof(metadata) = 'object');

CREATE TABLE assets (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    current_version_id BIGINT,
    origin_upload_id BIGINT UNIQUE REFERENCES uploads(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    CONSTRAINT assets_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT assets_media_type_valid
        CHECK (media_type IN ('image', 'audio', 'video', 'live2d', 'font', 'other')),
    CONSTRAINT assets_status_valid CHECK (status IN ('active', 'archived', 'deleting')),
    CONSTRAINT assets_deleted_state_consistent
        CHECK ((status = 'deleting') = (deleted_at IS NOT NULL))
);

CREATE TABLE asset_versions (
    id BIGSERIAL PRIMARY KEY,
    asset_id BIGINT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    version_no INTEGER NOT NULL,
    upload_id BIGINT UNIQUE NOT NULL REFERENCES uploads(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT asset_versions_number_positive CHECK (version_no > 0),
    CONSTRAINT asset_versions_asset_number_unique UNIQUE (asset_id, version_no)
);

ALTER TABLE assets
    ADD CONSTRAINT assets_current_version_fk
    FOREIGN KEY (current_version_id)
    REFERENCES asset_versions(id)
    ON DELETE SET NULL
    DEFERRABLE INITIALLY DEFERRED;

CREATE OR REPLACE FUNCTION validate_asset_current_version()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.current_version_id IS NOT NULL AND NOT EXISTS (
        SELECT 1
        FROM asset_versions version
        WHERE version.id = NEW.current_version_id
          AND version.asset_id = NEW.id
    ) THEN
        RAISE EXCEPTION 'asset current_version_id must reference a version of the same asset';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_assets_validate_current_version
BEFORE INSERT OR UPDATE OF current_version_id ON assets
FOR EACH ROW EXECUTE FUNCTION validate_asset_current_version();

CREATE TRIGGER trg_assets_updated_at
BEFORE UPDATE ON assets
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX idx_assets_library
    ON assets (status, media_type, created_at DESC, id DESC);
CREATE INDEX idx_assets_name_trgm
    ON assets USING GIN (name gin_trgm_ops);
CREATE INDEX idx_asset_versions_history
    ON asset_versions (asset_id, version_no DESC);

-- Upgrade existing upload rows into one-version logical assets without moving
-- their MinIO objects or changing object keys.
INSERT INTO assets (name, media_type, origin_upload_id, created_at, updated_at)
SELECT
    regexp_replace(object_key, '^.*/', ''),
    CASE
        WHEN kind IN ('cover', 'article_image', 'avatar', 'moment_image', 'image') THEN 'image'
        WHEN kind IN ('bgm', 'voice', 'audio') THEN 'audio'
        WHEN kind = 'video' THEN 'video'
        WHEN kind = 'live2d' THEN 'live2d'
        WHEN kind = 'font' THEN 'font'
        ELSE 'other'
    END,
    id,
    created_at,
    created_at
FROM uploads
ON CONFLICT (origin_upload_id) DO NOTHING;

INSERT INTO asset_versions (asset_id, version_no, upload_id, created_at)
SELECT asset.id, 1, asset.origin_upload_id, asset.created_at
FROM assets asset
WHERE asset.origin_upload_id IS NOT NULL
ON CONFLICT (upload_id) DO NOTHING;

UPDATE assets asset
SET current_version_id = version.id
FROM asset_versions version
WHERE version.asset_id = asset.id
  AND version.version_no = 1
  AND asset.current_version_id IS NULL;

-- Typed references used directly by the current business tables.
ALTER TABLE articles
    ADD COLUMN cover_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;
ALTER TABLE theme_configs
    ADD COLUMN cover_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT,
    ADD COLUMN voice_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;
ALTER TABLE music_tracks
    ADD COLUMN file_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;
ALTER TABLE live2d_models
    ADD COLUMN asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;
ALTER TABLE games
    ADD COLUMN cover_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;
ALTER TABLE bangumi
    ADD COLUMN cover_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;
ALTER TABLE friends
    ADD COLUMN avatar_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;

CREATE UNIQUE INDEX idx_live2d_models_asset_unique
    ON live2d_models (asset_id)
    WHERE asset_id IS NOT NULL;

CREATE TABLE article_assets (
    article_id BIGINT NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    asset_id BIGINT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    role TEXT NOT NULL DEFAULT 'content',
    sort_order INTEGER NOT NULL DEFAULT 0,
    alt_text TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id, asset_id, role),
    CONSTRAINT article_assets_role_valid CHECK (role IN ('content', 'attachment')),
    CONSTRAINT article_assets_sort_non_negative CHECK (sort_order >= 0)
);

CREATE TABLE moment_assets (
    moment_id BIGINT NOT NULL REFERENCES moments(id) ON DELETE CASCADE,
    asset_id BIGINT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    alt_text TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (moment_id, asset_id),
    CONSTRAINT moment_assets_sort_non_negative CHECK (sort_order >= 0)
);

-- Flexible references cover JSON-backed settings and future modules without a
-- schema rewrite. `source_key` is a stable slot such as `site:about:avatar`.
CREATE TABLE asset_references (
    id BIGSERIAL PRIMARY KEY,
    asset_id BIGINT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    source_type TEXT NOT NULL,
    source_key TEXT NOT NULL,
    source_label TEXT NOT NULL DEFAULT '',
    admin_path TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT asset_references_source_type_not_blank CHECK (btrim(source_type) <> ''),
    CONSTRAINT asset_references_source_key_not_blank CHECK (btrim(source_key) <> ''),
    CONSTRAINT asset_references_source_unique UNIQUE (source_type, source_key)
);

CREATE INDEX idx_asset_references_asset ON asset_references (asset_id);
CREATE TRIGGER trg_asset_references_updated_at
BEFORE UPDATE ON asset_references
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- A single view powers reference counts and the “used by” list in the asset
-- detail dialog. Paths are hints for the admin frontend, not authorization data.
CREATE VIEW asset_usage AS
SELECT cover_asset_id AS asset_id, 'article_cover'::TEXT AS source_type,
       id::TEXT AS source_id, title AS source_label,
       '/admin/articles/' || id || '/edit' AS admin_path
FROM articles WHERE cover_asset_id IS NOT NULL
UNION ALL
SELECT link.asset_id, 'article_content', link.article_id::TEXT,
       article.title, '/admin/articles/' || article.id || '/edit'
FROM article_assets link JOIN articles article ON article.id = link.article_id
UNION ALL
SELECT cover_asset_id, 'theme_cover', mode, mode || ' theme cover', '/admin/appearance'
FROM theme_configs WHERE cover_asset_id IS NOT NULL
UNION ALL
SELECT voice_asset_id, 'theme_voice', mode, mode || ' opening voice', '/admin/media'
FROM theme_configs WHERE voice_asset_id IS NOT NULL
UNION ALL
SELECT file_asset_id, 'music_track', id::TEXT, title, '/admin/media'
FROM music_tracks WHERE file_asset_id IS NOT NULL
UNION ALL
SELECT asset_id, 'live2d_model', id::TEXT, name, '/admin/kanban'
FROM live2d_models WHERE asset_id IS NOT NULL
UNION ALL
SELECT link.asset_id, 'moment_image', link.moment_id::TEXT,
       left(moment.content, 80), '/admin/moments/' || moment.id
FROM moment_assets link JOIN moments moment ON moment.id = link.moment_id
UNION ALL
SELECT cover_asset_id, 'game_cover', id::TEXT, title, '/admin/games/' || id
FROM games WHERE cover_asset_id IS NOT NULL
UNION ALL
SELECT cover_asset_id, 'bangumi_cover', id::TEXT, title, '/admin/settings'
FROM bangumi WHERE cover_asset_id IS NOT NULL
UNION ALL
SELECT avatar_asset_id, 'friend_avatar', id::TEXT, name, '/admin/friends/' || id
FROM friends WHERE avatar_asset_id IS NOT NULL
UNION ALL
SELECT asset_id, source_type, source_key, source_label, admin_path
FROM asset_references;

CREATE VIEW asset_usage_counts AS
SELECT asset_id, COUNT(*)::BIGINT AS reference_count
FROM asset_usage
GROUP BY asset_id;

-- Editable resources need an update timestamp for optimistic UI refreshes.
ALTER TABLE categories ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE tags ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE moments ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE games ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();
ALTER TABLE friends ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE TRIGGER trg_categories_updated_at
BEFORE UPDATE ON categories
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_tags_updated_at
BEFORE UPDATE ON tags
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_moments_updated_at
BEFORE UPDATE ON moments
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_games_updated_at
BEFORE UPDATE ON games
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
CREATE TRIGGER trg_friends_updated_at
BEFORE UPDATE ON friends
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
