-- Playlists are a reusable content catalogue, not a site-wide player.
-- Remove the obsolete autoplay and volume settings left by migration 0023.
UPDATE site_settings
SET settings = settings - 'playlist'
WHERE id = 1;
