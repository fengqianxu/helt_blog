-- Persist the home-cover eyebrow with the other editable branding fields while
-- retaining the original copy for existing installations.
UPDATE site_settings
SET settings = jsonb_set(
    settings,
    '{basic,hero_eyebrow}',
    COALESCE(
        settings #> '{basic,hero_eyebrow}',
        to_jsonb('SINCE 2020 · HELT''S BLOG'::text)
    ),
    true
)
WHERE id = 1;
