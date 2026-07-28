-- Complete the one-time comment reset without removing Artalk's configured
-- administrator or site record. These tables hold commenter/page metadata even
-- after every comment has been deleted.
DO $$
BEGIN
    IF to_regclass('artalk_auth_identities') IS NOT NULL THEN
        DELETE FROM artalk_auth_identities;
    END IF;
    IF to_regclass('artalk_user_email_verifies') IS NOT NULL THEN
        DELETE FROM artalk_user_email_verifies;
    END IF;
    IF to_regclass('artalk_pages') IS NOT NULL THEN
        DELETE FROM artalk_pages;
    END IF;
    IF to_regclass('artalk_users') IS NOT NULL THEN
        DELETE FROM artalk_users
        WHERE NOT COALESCE(is_admin, FALSE);
    END IF;
END
$$;
