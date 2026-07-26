-- A raiment owns appearance and cover content. Time-based activation belongs to
-- the singleton site settings document and references raiments by stable id.

ALTER TABLE raiments
    ADD COLUMN cover_title TEXT NOT NULL DEFAULT '',
    ADD COLUMN cover_subtitle TEXT NOT NULL DEFAULT '',
    ADD COLUMN cover_character_name TEXT NOT NULL DEFAULT '',
    ADD COLUMN cover_dialogue TEXT NOT NULL DEFAULT '',
    ADD COLUMN cover_voice_label TEXT NOT NULL DEFAULT '',
    ADD COLUMN cover_voice_asset_id BIGINT REFERENCES assets(id) ON DELETE SET NULL,
    ADD CONSTRAINT raiments_cover_title_length CHECK (char_length(cover_title) <= 240),
    ADD CONSTRAINT raiments_cover_subtitle_length CHECK (char_length(cover_subtitle) <= 240),
    ADD CONSTRAINT raiments_cover_character_name_length CHECK (char_length(cover_character_name) <= 80),
    ADD CONSTRAINT raiments_cover_dialogue_length CHECK (char_length(cover_dialogue) <= 500),
    ADD CONSTRAINT raiments_cover_voice_label_length CHECK (char_length(cover_voice_label) <= 120);

UPDATE raiments
SET cover_title = '「問おう。' || chr(10) || '貴方が私のマスターか？」',
    cover_subtitle = '—— 我问你，你就是我的 Master 吗？',
    cover_character_name = CASE WHEN id = 'alter-saber' THEN 'Alter' ELSE 'Saber' END,
    cover_dialogue = CASE
        WHEN id = 'alter-saber' THEN '夜已深，Master。仍要继续前行吗？'
        ELSE '今日もいい天気ですね。'
    END,
    cover_voice_label = CASE
        WHEN id = 'alter-saber' THEN '音声を再生 · Alter'
        ELSE '音声を再生 · 川澄綾子'
    END,
    cover_voice_asset_id = CASE
        WHEN id = 'alter-saber' THEN (
            SELECT asset.id
            FROM assets asset
            JOIN uploads upload ON upload.id = asset.upload_id
            WHERE upload.object_key = 'voice/login/alter-saber.mp3'
        )
        WHEN id = 'saber' THEN (
            SELECT asset.id
            FROM assets asset
            JOIN uploads upload ON upload.id = asset.upload_id
            WHERE upload.object_key = 'voice/login/blue-saber.mp3'
        )
        ELSE NULL
    END;

-- Promote the small v1 palette to the complete set of colors that changes with
-- a raiment. Existing custom values remain authoritative for the first three.
UPDATE raiments
SET theme = theme || CASE color_scheme
    WHEN 'night' THEN '{
        "surface":"#171320",
        "surface_alt":"#211B2B",
        "text":"#EAE7F2",
        "text_secondary":"#C5BFD1",
        "muted":"#9A94AD",
        "faint":"#6F6A80",
        "border":"#3A3447",
        "danger":"#F0718A",
        "success":"#77B989"
    }'::jsonb
    ELSE '{
        "surface":"#FFFFFF",
        "surface_alt":"#F0EFE9",
        "text":"#1F2534",
        "text_secondary":"#3A4155",
        "muted":"#6B7284",
        "faint":"#9AA1B3",
        "border":"#D9DCE3",
        "danger":"#D84358",
        "success":"#3D8455"
    }'::jsonb
END;

-- Convert the old point-in-time switches into adjacent time ranges before the
-- obsolete column is removed. The final range wraps to the first start time.
WITH ordered AS (
    SELECT
        id,
        switch_at AS start_at,
        lead(switch_at) OVER (ORDER BY switch_at, created_at, id) AS next_at,
        min(switch_at) OVER () AS first_at
    FROM raiments
    WHERE enabled = true
), schedule AS (
    SELECT jsonb_agg(
        jsonb_build_object(
            'id', 'period-' || id,
            'start_at', to_char(start_at, 'HH24:MI'),
            'end_at', to_char(COALESCE(next_at, first_at), 'HH24:MI'),
            'raiment_id', id
        )
        ORDER BY start_at, id
    ) AS periods
    FROM ordered
)
UPDATE site_settings
SET settings = jsonb_set(
    settings,
    '{raiment_schedule}',
    jsonb_build_object(
        'revision', 1,
        'periods', COALESCE((SELECT periods FROM schedule), '[]'::jsonb)
    ),
    true
)
WHERE id = 1;

DROP INDEX IF EXISTS idx_raiments_switch_at_unique;
DROP INDEX IF EXISTS idx_raiments_schedule;
ALTER TABLE raiments DROP COLUMN switch_at;
CREATE INDEX idx_raiments_public_order ON raiments (enabled, created_at, id);

INSERT INTO asset_references (
    asset_id, source_type, source_key, source_label, admin_path
)
SELECT
    cover_voice_asset_id,
    'raiment_voice',
    id,
    name || ' 灵衣封面语音',
    '/admin/raiments'
FROM raiments
WHERE cover_voice_asset_id IS NOT NULL
ON CONFLICT (source_type, source_key) DO UPDATE
SET asset_id = EXCLUDED.asset_id,
    source_label = EXCLUDED.source_label,
    admin_path = EXCLUDED.admin_path;
