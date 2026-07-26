-- Raiment is the persisted owner of the public cover and theme tokens.
-- Kanban persona, chat and Live2D remain outside this migration intentionally.

INSERT INTO uploads (
    object_key, bucket, mime, size_bytes, kind,
    original_filename, checksum_sha256, metadata, created_at
)
VALUES
    (
        'raiments/saber/cover.png', 'blog-public', 'image/png', 7636360, 'image',
        'saber-day.png',
        'cf404334b5538619ac9006b409f3eeb56728343a3c3bbad99a29c57c502c24a1',
        '{"managed_by":"bootstrap","role":"raiment_cover","raiment":"saber"}'::jsonb,
        now()
    ),
    (
        'raiments/alter-saber/cover.png', 'blog-public', 'image/png', 8057609, 'image',
        'saber-night.png',
        '6bdd7c054676af2e70756cf550ca30490bc2cf6d32b2e35dda942fd085addb57',
        '{"managed_by":"bootstrap","role":"raiment_cover","raiment":"alter-saber"}'::jsonb,
        now()
    )
ON CONFLICT (object_key) DO NOTHING;

INSERT INTO assets (name, media_type, upload_id, created_at, updated_at)
SELECT seed.name, 'image', upload.id, upload.created_at, upload.created_at
FROM (
    VALUES
        ('raiments/saber/cover.png', 'Saber 日间灵衣封面'),
        ('raiments/alter-saber/cover.png', 'Alter Saber 夜间灵衣封面')
) AS seed(object_key, name)
JOIN uploads upload ON upload.object_key = seed.object_key
ON CONFLICT (upload_id) DO NOTHING;

CREATE TABLE raiments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    cover_asset_id BIGINT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT,
    theme JSONB NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    revision BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT raiments_id_valid CHECK (id ~ '^[a-z0-9]+(?:-[a-z0-9]+)*$'),
    CONSTRAINT raiments_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT raiments_theme_object CHECK (jsonb_typeof(theme) = 'object'),
    CONSTRAINT raiments_revision_positive CHECK (revision > 0)
);

CREATE TRIGGER trg_raiments_updated_at
BEFORE UPDATE ON raiments
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX idx_raiments_public
    ON raiments (enabled, created_at, id);

INSERT INTO raiments (id, name, cover_asset_id, theme)
SELECT
    seed.id,
    seed.name,
    asset.id,
    seed.theme
FROM (
    VALUES
        (
            'saber',
            'Saber',
            'raiments/saber/cover.png',
            '{"primary":"#2B5FB8","secondary":"#B99A3E","background":"#F5F7FB"}'::jsonb
        ),
        (
            'alter-saber',
            'Alter Saber',
            'raiments/alter-saber/cover.png',
            '{"primary":"#D84358","secondary":"#7B4B8E","background":"#0E0B16"}'::jsonb
        )
) AS seed(id, name, object_key, theme)
JOIN uploads upload ON upload.object_key = seed.object_key
JOIN assets asset ON asset.upload_id = upload.id
ON CONFLICT (id) DO NOTHING;

INSERT INTO asset_references (
    asset_id, source_type, source_key, source_label, admin_path
)
SELECT
    cover_asset_id,
    'raiment_cover',
    id,
    name || ' 灵衣封面',
    '/admin/raiments'
FROM raiments
ON CONFLICT (source_type, source_key) DO UPDATE
SET asset_id = EXCLUDED.asset_id,
    source_label = EXCLUDED.source_label,
    admin_path = EXCLUDED.admin_path;

UPDATE site_settings
SET settings = jsonb_set(
    settings,
    '{raiments}',
    jsonb_build_object(
        'bindings', jsonb_build_object(
            'day', jsonb_build_array('saber'),
            'night', jsonb_build_array('alter-saber')
        ),
        'rule', jsonb_build_object(
            'follow_system', COALESCE((settings #>> '{theme,follow_system}')::boolean, true),
            'schedule', COALESCE(
                settings #> '{theme,schedule}',
                '{"day_at":"07:00","night_at":"19:00"}'::jsonb
            )
        )
    ),
    true
)
WHERE id = 1;
