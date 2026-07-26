ALTER TABLE games
    ADD COLUMN steam_app_id BIGINT,
    ADD COLUMN icon_hash TEXT NOT NULL DEFAULT '',
    ADD COLUMN playtime_2weeks_minutes INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN playtime_forever_minutes INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN playtime_windows_minutes INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN playtime_mac_minutes INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN playtime_linux_minutes INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN last_played_at TIMESTAMPTZ,
    ADD COLUMN synced_at TIMESTAMPTZ;

ALTER TABLE games
    ADD CONSTRAINT games_steam_app_id_positive
        CHECK (steam_app_id IS NULL OR steam_app_id > 0),
    ADD CONSTRAINT games_steam_playtime_non_negative
        CHECK (
            playtime_2weeks_minutes >= 0
            AND playtime_forever_minutes >= 0
            AND playtime_windows_minutes >= 0
            AND playtime_mac_minutes >= 0
            AND playtime_linux_minutes >= 0
        );

CREATE UNIQUE INDEX idx_games_steam_app_id
    ON games (steam_app_id)
    WHERE steam_app_id IS NOT NULL;

CREATE INDEX idx_games_steam_recent
    ON games (last_played_at DESC NULLS LAST, playtime_forever_minutes DESC)
    WHERE steam_app_id IS NOT NULL;

UPDATE site_settings
SET settings = jsonb_set(
    settings,
    '{steam_sync}',
    COALESCE(settings -> 'steam_sync', '{}'::jsonb) || '{
      "web_api_key": "",
      "steam_id64": "",
      "interval_hours": 6,
      "last_sync_at": null,
      "last_status": null,
      "last_counts": {"total": 0, "recent": 0}
    }'::jsonb,
    true
), updated_at = now()
WHERE id = 1;
