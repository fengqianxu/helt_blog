-- Make the footer's metadata line editable while preserving the text that was
-- previously rendered by the frontend. The placeholders are expanded there.
UPDATE site_settings
SET settings = jsonb_set(
    settings,
    '{basic,footer_copyright}',
    COALESCE(
        settings #> '{basic,footer_copyright}',
        '"© 2020—{year} {site_name} · POWERED BY REACT"'::jsonb
    ),
    true
)
WHERE id = 1;
