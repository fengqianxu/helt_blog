-- Replace the flat BGM list with first-class playlists. Existing tracks are
-- preserved inside one local playlist, so upgrades do not lose references to
-- logical assets or their MinIO objects.

CREATE TABLE playlists (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    source_kind TEXT NOT NULL DEFAULT 'local',
    external_id TEXT,
    external_url TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT playlists_name_not_blank CHECK (btrim(name) <> ''),
    CONSTRAINT playlists_source_valid CHECK (source_kind IN ('local', 'netease', 'qq')),
    CONSTRAINT playlists_external_shape CHECK (
        (source_kind = 'local' AND external_id IS NULL AND external_url IS NULL)
        OR
        (source_kind IN ('netease', 'qq')
            AND btrim(COALESCE(external_id, '')) <> ''
            AND btrim(COALESCE(external_url, '')) <> '')
    ),
    CONSTRAINT playlists_sort_non_negative CHECK (sort_order >= 0),
    CONSTRAINT playlists_external_unique UNIQUE (source_kind, external_id)
);

CREATE TRIGGER trg_playlists_updated_at
BEFORE UPDATE ON playlists
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE INDEX idx_playlists_order ON playlists (sort_order, id);

INSERT INTO playlists (name, description, source_kind, enabled, sort_order)
VALUES ('站点歌单', '从原背景音乐列表迁移', 'local', true, 0);

ALTER TABLE music_tracks RENAME TO playlist_tracks;
ALTER TABLE playlist_tracks
    ADD COLUMN playlist_id BIGINT REFERENCES playlists(id) ON DELETE CASCADE,
    ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

UPDATE playlist_tracks
SET playlist_id = (SELECT id FROM playlists ORDER BY id LIMIT 1)
WHERE playlist_id IS NULL;

ALTER TABLE playlist_tracks ALTER COLUMN playlist_id SET NOT NULL;
ALTER INDEX idx_music_tracks_order RENAME TO idx_playlist_tracks_legacy_order;
CREATE INDEX idx_playlist_tracks_order
    ON playlist_tracks (playlist_id, sort_order, id);
CREATE UNIQUE INDEX idx_playlist_tracks_asset_unique
    ON playlist_tracks (playlist_id, file_asset_id)
    WHERE file_asset_id IS NOT NULL;

CREATE TRIGGER trg_playlist_tracks_updated_at
BEFORE UPDATE ON playlist_tracks
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Keep the former music settings as input during upgrade, but expose them
-- under the new playlist key from now on.
UPDATE site_settings
SET settings = jsonb_set(
    settings - 'music',
    '{playlist}',
    COALESCE(settings -> 'playlist', settings -> 'music', '{"autoplay":false,"default_volume":0.5}'::jsonb),
    true
)
WHERE id = 1;
