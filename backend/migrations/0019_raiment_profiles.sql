-- Every raiment is now a self-contained scheduled profile. The two seeded
-- profiles remain marked as built-ins, but the API deliberately allows them
-- to be renamed, edited and deleted like profiles created later.

ALTER TABLE raiments
    ADD COLUMN switch_at TIME NOT NULL DEFAULT '00:00',
    ADD COLUMN color_scheme TEXT NOT NULL DEFAULT 'day',
    ADD COLUMN kanban_asset_id BIGINT REFERENCES assets(id) ON DELETE SET NULL,
    ADD COLUMN is_builtin BOOLEAN NOT NULL DEFAULT false,
    ADD CONSTRAINT raiments_color_scheme_valid
        CHECK (color_scheme IN ('day', 'night'));

UPDATE raiments
SET name = CASE WHEN name = 'Saber' THEN '日间模式' ELSE name END,
    switch_at = COALESCE(
        (SELECT (settings #>> '{raiments,rule,schedule,day_at}')::time
         FROM site_settings WHERE id = 1),
        '07:00'::time
    ),
    color_scheme = 'day',
    is_builtin = true
WHERE id = 'saber';

UPDATE raiments
SET name = CASE WHEN name = 'Alter Saber' THEN '夜间模式' ELSE name END,
    switch_at = COALESCE(
        (SELECT (settings #>> '{raiments,rule,schedule,night_at}')::time
         FROM site_settings WHERE id = 1),
        '19:00'::time
    ),
    color_scheme = 'night',
    is_builtin = true
WHERE id = 'alter-saber';

CREATE UNIQUE INDEX idx_raiments_switch_at_unique ON raiments (switch_at);
CREATE INDEX idx_raiments_schedule ON raiments (enabled, switch_at, created_at, id);

-- bindings/rule belonged to the old two-layer model. Scheduling now lives on
-- each raiment row, so the stale site setting must not remain authoritative.
UPDATE site_settings
SET settings = settings - 'raiments'
WHERE id = 1;
