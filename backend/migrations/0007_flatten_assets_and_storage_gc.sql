-- Assets point directly at their only physical upload. Replacing an asset swaps
-- this pointer and schedules the previous object for durable asynchronous
-- deletion; no version history or rollback model remains.

ALTER TABLE assets
    ADD COLUMN upload_id BIGINT REFERENCES uploads(id) ON DELETE RESTRICT;

UPDATE assets asset
SET upload_id = version.upload_id
FROM asset_versions version
WHERE version.id = asset.current_version_id;

ALTER TABLE assets
    ALTER COLUMN upload_id SET NOT NULL,
    ADD CONSTRAINT assets_upload_unique UNIQUE (upload_id);

DROP TRIGGER IF EXISTS trg_assets_validate_current_version ON assets;
DROP FUNCTION IF EXISTS validate_asset_current_version();
ALTER TABLE assets
    DROP CONSTRAINT IF EXISTS assets_current_version_fk,
    DROP COLUMN current_version_id,
    DROP COLUMN origin_upload_id;
DROP TABLE asset_versions;

CREATE TABLE storage_gc_jobs (
    id BIGSERIAL PRIMARY KEY,
    object_key TEXT UNIQUE NOT NULL,
    reason TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_at TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT storage_gc_object_key_not_blank CHECK (btrim(object_key) <> ''),
    CONSTRAINT storage_gc_reason_not_blank CHECK (btrim(reason) <> ''),
    CONSTRAINT storage_gc_attempts_non_negative CHECK (attempts >= 0)
);

CREATE INDEX idx_storage_gc_due
    ON storage_gc_jobs (next_attempt_at, id);

CREATE TRIGGER trg_storage_gc_updated_at
BEFORE UPDATE ON storage_gc_jobs
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
