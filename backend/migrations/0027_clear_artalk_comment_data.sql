-- One-time reset requested when replacing the dashboard comment widget with
-- the live moderation queue. Artalk can create its tables after the backend on
-- a fresh install, so each cleanup is conditional and therefore migration-safe.
DO $$
BEGIN
    IF to_regclass('artalk_notifies') IS NOT NULL THEN
        DELETE FROM artalk_notifies;
    END IF;
    IF to_regclass('artalk_votes') IS NOT NULL THEN
        DELETE FROM artalk_votes;
    END IF;
    IF to_regclass('artalk_comments') IS NOT NULL THEN
        DELETE FROM artalk_comments;
    END IF;
END
$$;
