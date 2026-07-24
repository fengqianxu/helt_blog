-- Register the public objects that are provisioned outside the upload API.
-- Object keys are the idempotency boundary, so this migration is safe after a
-- manual import and does not create duplicate logical assets.

INSERT INTO uploads (
    object_key, bucket, mime, size_bytes, kind,
    original_filename, checksum_sha256, metadata, created_at
)
VALUES
    (
        'avatars/default/admin-avatar.webp', 'blog-public', 'image/webp', 51654, 'image',
        'admin-avatar.webp',
        '460ebd8e82fe04adfffb6535f82b095a2c7dfe8bd87199bcd1e927c3ab598b2d',
        '{"managed_by":"bootstrap","role":"default_admin_avatar"}'::jsonb,
        '2026-07-24T02:15:33Z'::timestamptz
    ),
    (
        'voice/login/alter-saber-success.mp3', 'blog-public', 'audio/mpeg', 11766, 'audio',
        'alter-saber-success.mp3',
        'd2db2f7e607cc140b4bb619dde83237b8cb59eab53c42ba44c98d6dc65a51d35',
        '{"managed_by":"bootstrap","role":"login_success_voice","raiment":"alter-saber"}'::jsonb,
        '2026-07-23T13:20:32Z'::timestamptz
    ),
    (
        'voice/login/alter-saber.mp3', 'blog-public', 'audio/mpeg', 224652, 'audio',
        'alter-saber.mp3',
        '8ab2d25a5d8b1d428e1ea38e82bd774a75c554807c3b6cde61235a2fdd98e3d8',
        '{"managed_by":"bootstrap","role":"login_voice","raiment":"alter-saber"}'::jsonb,
        '2026-07-23T12:22:54Z'::timestamptz
    ),
    (
        'voice/login/blue-saber-success.mp3', 'blog-public', 'audio/mpeg', 86725, 'audio',
        'blue-saber-success.mp3',
        '6542875ddf635f3794b4ad76a91283572a38ad9c40e1e5025019d43ed7e06a8b',
        '{"managed_by":"bootstrap","role":"login_success_voice","raiment":"saber"}'::jsonb,
        '2026-07-23T13:20:32Z'::timestamptz
    ),
    (
        'voice/login/blue-saber.mp3', 'blog-public', 'audio/mpeg', 133746, 'audio',
        'blue-saber.mp3',
        '18abca43ef213d0baecda551a4867b0dcb69894990b1b6c5d9d10daf62f504f5',
        '{"managed_by":"bootstrap","role":"login_voice","raiment":"saber"}'::jsonb,
        '2026-07-23T12:22:54Z'::timestamptz
    )
ON CONFLICT (object_key) DO NOTHING;

INSERT INTO assets (name, media_type, origin_upload_id, created_at, updated_at)
SELECT seed.name, seed.media_type, upload.id, upload.created_at, upload.created_at
FROM (
    VALUES
        ('avatars/default/admin-avatar.webp', '默认管理员头像', 'image'),
        ('voice/login/alter-saber-success.mp3', 'Alter Saber 登录成功语音', 'audio'),
        ('voice/login/alter-saber.mp3', 'Alter Saber 登录语音', 'audio'),
        ('voice/login/blue-saber-success.mp3', 'Saber 登录成功语音', 'audio'),
        ('voice/login/blue-saber.mp3', 'Saber 登录语音', 'audio')
) AS seed(object_key, name, media_type)
JOIN uploads upload ON upload.object_key = seed.object_key
ON CONFLICT (origin_upload_id) DO NOTHING;

INSERT INTO asset_versions (asset_id, version_no, upload_id, created_at)
SELECT asset.id, 1, asset.origin_upload_id, asset.created_at
FROM assets asset
JOIN uploads upload ON upload.id = asset.origin_upload_id
WHERE upload.object_key IN (
    'avatars/default/admin-avatar.webp',
    'voice/login/alter-saber-success.mp3',
    'voice/login/alter-saber.mp3',
    'voice/login/blue-saber-success.mp3',
    'voice/login/blue-saber.mp3'
)
ON CONFLICT (upload_id) DO NOTHING;

UPDATE assets asset
SET current_version_id = version.id
FROM asset_versions version
JOIN uploads upload ON upload.id = version.upload_id
WHERE version.asset_id = asset.id
  AND asset.current_version_id IS NULL
  AND upload.object_key IN (
      'avatars/default/admin-avatar.webp',
      'voice/login/alter-saber-success.mp3',
      'voice/login/alter-saber.mp3',
      'voice/login/blue-saber-success.mp3',
      'voice/login/blue-saber.mp3'
  );

INSERT INTO asset_references (
    asset_id, source_type, source_key, source_label, admin_path
)
SELECT asset.id, seed.source_type, seed.source_key, seed.source_label, seed.admin_path
FROM (
    VALUES
        ('avatars/default/admin-avatar.webp', 'system_default', 'admin:default-avatar', '管理员默认头像', '/admin/profile'),
        ('voice/login/alter-saber-success.mp3', 'login_voice', 'alter-saber:success', 'Alter Saber 登录成功语音', '/admin/media'),
        ('voice/login/alter-saber.mp3', 'login_voice', 'alter-saber:prompt', 'Alter Saber 登录语音', '/admin/media'),
        ('voice/login/blue-saber-success.mp3', 'login_voice', 'saber:success', 'Saber 登录成功语音', '/admin/media'),
        ('voice/login/blue-saber.mp3', 'login_voice', 'saber:prompt', 'Saber 登录语音', '/admin/media')
) AS seed(object_key, source_type, source_key, source_label, admin_path)
JOIN uploads upload ON upload.object_key = seed.object_key
JOIN assets asset ON asset.origin_upload_id = upload.id
ON CONFLICT (source_type, source_key) DO UPDATE
SET asset_id = EXCLUDED.asset_id,
    source_label = EXCLUDED.source_label,
    admin_path = EXCLUDED.admin_path;
