ALTER TABLE admin_users
    ADD COLUMN avatar_asset_id BIGINT REFERENCES assets(id) ON DELETE RESTRICT;

CREATE UNIQUE INDEX idx_admin_users_avatar_asset_unique
    ON admin_users (avatar_asset_id)
    WHERE avatar_asset_id IS NOT NULL;
