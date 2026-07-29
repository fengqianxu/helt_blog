-- Bootstrap the four social icons copied into MinIO by storage-init. Existing
-- social links with a recognizable label are upgraded to reference the matching
-- asset; later edits keep the references in sync in the profile API.

INSERT INTO uploads (
    object_key, bucket, mime, size_bytes, kind,
    original_filename, checksum_sha256, metadata, created_at
)
VALUES
    (
        'icons/social/bilibili.svg', 'blog-public', 'image/svg+xml', 510, 'image',
        'bilibili.svg',
        '75f344a54e9557f0862e6333994fe2e83a7290559d4f616a27847d5867752861',
        '{"managed_by":"bootstrap","role":"social_icon","provider":"bilibili"}'::jsonb,
        '2026-07-29T00:00:00Z'::timestamptz
    ),
    (
        'icons/social/steam.svg', 'blog-public', 'image/svg+xml', 502, 'image',
        'steam.svg',
        'f4fd48ecd12e093de2aef4ee1a647674e6ef0368e91c1a69bdb2732580db987e',
        '{"managed_by":"bootstrap","role":"social_icon","provider":"steam"}'::jsonb,
        '2026-07-29T00:00:00Z'::timestamptz
    ),
    (
        'icons/social/github.svg', 'blog-public', 'image/svg+xml', 547, 'image',
        'github.svg',
        '04a41f073d71ed903d6c52abddf7ce0af0a38ae3f31acecd9eee65dac6a8a017',
        '{"managed_by":"bootstrap","role":"social_icon","provider":"github"}'::jsonb,
        '2026-07-29T00:00:00Z'::timestamptz
    ),
    (
        'icons/social/email.svg', 'blog-public', 'image/svg+xml', 474, 'image',
        'email.svg',
        'a0e8189efb8e9b376f313e120dce0f3462f0adf22138c63ba5023fb565e5c532',
        '{"managed_by":"bootstrap","role":"social_icon","provider":"email"}'::jsonb,
        '2026-07-29T00:00:00Z'::timestamptz
    )
ON CONFLICT (object_key) DO NOTHING;

INSERT INTO assets (name, media_type, upload_id, created_at, updated_at)
SELECT seed.name, 'image', upload.id, upload.created_at, upload.created_at
FROM (
    VALUES
        ('icons/social/bilibili.svg', 'Bilibili 社交图标'),
        ('icons/social/steam.svg', 'Steam 社交图标'),
        ('icons/social/github.svg', 'GitHub 社交图标'),
        ('icons/social/email.svg', 'Email 社交图标')
) AS seed(object_key, name)
JOIN uploads upload ON upload.object_key = seed.object_key
ON CONFLICT (upload_id) DO NOTHING;

INSERT INTO asset_references (
    asset_id, source_type, source_key, source_label, admin_path
)
SELECT asset.id, 'system_social_icon', 'social-icon:' || seed.provider,
       seed.name, '/admin/profile'
FROM (
    VALUES
        ('icons/social/bilibili.svg', 'bilibili', 'Bilibili 社交图标'),
        ('icons/social/steam.svg', 'steam', 'Steam 社交图标'),
        ('icons/social/github.svg', 'github', 'GitHub 社交图标'),
        ('icons/social/email.svg', 'email', 'Email 社交图标')
) AS seed(object_key, provider, name)
JOIN uploads upload ON upload.object_key = seed.object_key
JOIN assets asset ON asset.upload_id = upload.id
ON CONFLICT (source_type, source_key) DO UPDATE
SET asset_id = EXCLUDED.asset_id,
    source_label = EXCLUDED.source_label,
    admin_path = EXCLUDED.admin_path;

WITH social_icons AS (
    SELECT seed.provider, asset.id
    FROM (
        VALUES
            ('icons/social/bilibili.svg', 'bilibili'),
            ('icons/social/steam.svg', 'steam'),
            ('icons/social/github.svg', 'github'),
            ('icons/social/email.svg', 'email')
    ) AS seed(object_key, provider)
    JOIN uploads upload ON upload.object_key = seed.object_key
    JOIN assets asset ON asset.upload_id = upload.id
)
UPDATE site_settings settings_row
SET settings = jsonb_set(
    settings_row.settings,
    '{about,socials}',
    COALESCE((
        SELECT jsonb_agg(
            CASE
                WHEN social.value ? 'icon_asset_id' OR icon.id IS NULL THEN social.value
                ELSE social.value || jsonb_build_object('icon_asset_id', icon.id)
            END
            ORDER BY social.ordinality
        )
        FROM jsonb_array_elements(
            COALESCE(settings_row.settings #> '{about,socials}', '[]'::jsonb)
        ) WITH ORDINALITY AS social(value, ordinality)
        LEFT JOIN social_icons icon ON icon.provider = CASE
            WHEN lower(btrim(social.value ->> 'label')) IN ('bilibili', 'b站', '哔哩哔哩') THEN 'bilibili'
            WHEN lower(btrim(social.value ->> 'label')) = 'steam' THEN 'steam'
            WHEN lower(btrim(social.value ->> 'label')) = 'github' THEN 'github'
            WHEN lower(btrim(social.value ->> 'label')) IN ('email', 'e-mail', '邮箱', '邮件') THEN 'email'
            ELSE NULL
        END
    ), '[]'::jsonb),
    true
)
WHERE jsonb_typeof(settings_row.settings #> '{about,socials}') = 'array';

INSERT INTO asset_references (
    asset_id, source_type, source_key, source_label, admin_path
)
SELECT (social.value ->> 'icon_asset_id')::bigint,
       'profile_social_icon',
       'site:about:social:' || (social.ordinality - 1),
       COALESCE(NULLIF(btrim(social.value ->> 'label'), ''), '社交链接') || ' 图标',
       '/admin/profile'
FROM site_settings settings_row
CROSS JOIN LATERAL jsonb_array_elements(
    COALESCE(settings_row.settings #> '{about,socials}', '[]'::jsonb)
) WITH ORDINALITY AS social(value, ordinality)
JOIN assets asset
  ON asset.id = CASE
      WHEN social.value ->> 'icon_asset_id' ~ '^[1-9][0-9]*$'
      THEN (social.value ->> 'icon_asset_id')::bigint
      ELSE NULL
  END
 AND asset.status = 'active'
 AND asset.media_type = 'image'
ON CONFLICT (source_type, source_key) DO UPDATE
SET asset_id = EXCLUDED.asset_id,
    source_label = EXCLUDED.source_label,
    admin_path = EXCLUDED.admin_path;
