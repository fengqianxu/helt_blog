-- Keep footer copy independent from the general site description. Existing
-- installations retain the text that was previously shown in the footer.
UPDATE site_settings
SET settings = jsonb_set(
    settings,
    '{basic,footer_text}',
    COALESCE(
        settings #> '{basic,footer_text}',
        settings #> '{basic,tagline}',
        '""'::jsonb
    ),
    true
)
WHERE id = 1;
