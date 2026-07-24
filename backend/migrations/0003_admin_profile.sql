ALTER TABLE admin_users
    ADD COLUMN email TEXT NOT NULL DEFAULT '',
    ADD COLUMN avatar_url TEXT;

ALTER TABLE admin_users
    ADD CONSTRAINT admin_users_email_length
        CHECK (char_length(email) <= 254),
    ADD CONSTRAINT admin_users_avatar_length
        CHECK (avatar_url IS NULL OR char_length(avatar_url) <= 1000000);
