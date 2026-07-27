-- Site branding is stored in the singleton settings document so deployments
-- can add a logo and browser icon without introducing another singleton table.
UPDATE site_settings
SET settings = jsonb_set(
    jsonb_set(
        jsonb_set(
            settings,
            '{basic,logo_asset_id}',
            COALESCE(settings #> '{basic,logo_asset_id}', 'null'::jsonb),
            true
        ),
        '{basic,favicon_asset_id}',
        COALESCE(settings #> '{basic,favicon_asset_id}', 'null'::jsonb),
        true
    ),
    '{features,splash}',
    COALESCE(
        settings #> '{features,splash}',
        settings #> '{theme,splash_enabled}',
        'true'::jsonb
    ),
    true
)
WHERE id = 1;
