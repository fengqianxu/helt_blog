ALTER TABLE admin_users
    ADD COLUMN avatar_crop_x REAL NOT NULL DEFAULT 0,
    ADD COLUMN avatar_crop_y REAL NOT NULL DEFAULT 0,
    ADD COLUMN avatar_crop_zoom REAL NOT NULL DEFAULT 1,
    ADD CONSTRAINT admin_avatar_crop_x_valid CHECK (avatar_crop_x BETWEEN -1 AND 1),
    ADD CONSTRAINT admin_avatar_crop_y_valid CHECK (avatar_crop_y BETWEEN -1 AND 1),
    ADD CONSTRAINT admin_avatar_crop_zoom_valid CHECK (avatar_crop_zoom BETWEEN 1 AND 3);
