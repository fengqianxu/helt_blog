-- Give public raiment selection a stable fallback/order and make the login
-- success voice part of the same persisted profile as the cover content.

ALTER TABLE raiments
    ADD COLUMN sort_order INTEGER,
    ADD COLUMN is_default BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN login_success_voice_asset_id BIGINT REFERENCES assets(id) ON DELETE SET NULL;

WITH ranked AS (
    SELECT
        id,
        (row_number() OVER (
            ORDER BY
                CASE id WHEN 'saber' THEN 0 WHEN 'alter-saber' THEN 1 ELSE 2 END,
                created_at,
                id
        ) - 1)::integer AS sort_order
    FROM raiments
)
UPDATE raiments
SET sort_order = ranked.sort_order
FROM ranked
WHERE raiments.id = ranked.id;

ALTER TABLE raiments
    ALTER COLUMN sort_order SET NOT NULL,
    ALTER COLUMN sort_order SET DEFAULT 0,
    ADD CONSTRAINT raiments_sort_order_nonnegative CHECK (sort_order >= 0),
    ADD CONSTRAINT raiments_default_enabled CHECK (NOT is_default OR enabled);

UPDATE raiments
SET is_default = id = (
    SELECT id
    FROM raiments
    WHERE enabled = true
    ORDER BY
        CASE id WHEN 'saber' THEN 0 WHEN 'alter-saber' THEN 1 ELSE 2 END,
        sort_order,
        created_at,
        id
    LIMIT 1
);

CREATE UNIQUE INDEX idx_raiments_single_default
    ON raiments (is_default)
    WHERE is_default = true;

DROP INDEX IF EXISTS idx_raiments_public;
DROP INDEX IF EXISTS idx_raiments_public_order;
CREATE INDEX idx_raiments_public_order
    ON raiments (enabled, sort_order, created_at, id);

UPDATE raiments
SET login_success_voice_asset_id = CASE
    WHEN id = 'alter-saber' THEN (
        SELECT asset.id
        FROM assets asset
        JOIN uploads upload ON upload.id = asset.upload_id
        WHERE upload.object_key = 'voice/login/alter-saber-success.mp3'
    )
    WHEN id = 'saber' THEN (
        SELECT asset.id
        FROM assets asset
        JOIN uploads upload ON upload.id = asset.upload_id
        WHERE upload.object_key = 'voice/login/blue-saber-success.mp3'
    )
    ELSE NULL
END;

INSERT INTO asset_references (
    asset_id, source_type, source_key, source_label, admin_path
)
SELECT
    login_success_voice_asset_id,
    'raiment_success_voice',
    id,
    name || ' 登录成功语音',
    '/admin/raiments'
FROM raiments
WHERE login_success_voice_asset_id IS NOT NULL
ON CONFLICT (source_type, source_key) DO UPDATE
SET asset_id = EXCLUDED.asset_id,
    source_label = EXCLUDED.source_label,
    admin_path = EXCLUDED.admin_path;
