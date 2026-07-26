\set ON_ERROR_STOP on

-- Safe, one-time repair for the known 0015 checksum fork. This script refuses
-- to touch SQLx history unless the recorded legacy checksum and every schema
-- object created by the repository migration match the audited expectations.
BEGIN;

LOCK TABLE _sqlx_migrations IN SHARE ROW EXCLUSIVE MODE;
LOCK TABLE bangumi IN ACCESS SHARE MODE;

DO $repair$
DECLARE
    recorded_checksum BYTEA;
    legacy_checksum CONSTANT BYTEA :=
        decode('a141c9d880eafd5312b9a585226d504c26ae9dd25e3707e64cb15f5f301313705f89281c91e61a367812c7611429520a', 'hex');
    repository_checksum CONSTANT BYTEA :=
        decode('620a821ba71550bfa5bd528c53f0892918600b8f3c25cff45cdb55f45aafc1c33865ee202392f8e78e3b195ca0b3ab3b', 'hex');
BEGIN
    SELECT checksum
    INTO recorded_checksum
    FROM _sqlx_migrations
    WHERE version = 15
      AND description = 'bangumi sync details'
      AND success;

    IF recorded_checksum IS NULL THEN
        RAISE EXCEPTION 'successful SQLx migration 15 was not found';
    END IF;

    IF recorded_checksum = repository_checksum THEN
        RAISE NOTICE 'migration 15 already has the repository checksum';
        RETURN;
    END IF;

    IF recorded_checksum <> legacy_checksum THEN
        RAISE EXCEPTION 'migration 15 has an unknown checksum: %',
            encode(recorded_checksum, 'hex');
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_attribute attribute
        JOIN pg_class relation ON relation.oid = attribute.attrelid
        JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
        JOIN pg_type data_type ON data_type.oid = attribute.atttypid
        WHERE namespace.nspname = 'public'
          AND relation.relname = 'bangumi'
          AND attribute.attname = 'season_id'
          AND NOT attribute.attnotnull
          AND data_type.typname = 'int8'
          AND attribute.attnum > 0
          AND NOT attribute.attisdropped
    ) THEN
        RAISE EXCEPTION 'bangumi.season_id does not match migration 0015';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_attribute attribute
        JOIN pg_class relation ON relation.oid = attribute.attrelid
        JOIN pg_namespace namespace ON namespace.oid = relation.relnamespace
        JOIN pg_type data_type ON data_type.oid = attribute.atttypid
        JOIN pg_attrdef default_value
          ON default_value.adrelid = attribute.attrelid
         AND default_value.adnum = attribute.attnum
        WHERE namespace.nspname = 'public'
          AND relation.relname = 'bangumi'
          AND attribute.attname = 'metadata'
          AND attribute.attnotnull
          AND data_type.typname = 'jsonb'
          AND pg_get_expr(default_value.adbin, default_value.adrelid) = '''{}''::jsonb'
          AND attribute.attnum > 0
          AND NOT attribute.attisdropped
    ) THEN
        RAISE EXCEPTION 'bangumi.metadata does not match migration 0015';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.bangumi'::regclass
          AND conname = 'bangumi_season_id_positive'
          AND pg_get_constraintdef(oid) =
              'CHECK (((season_id IS NULL) OR (season_id > 0)))'
    ) THEN
        RAISE EXCEPTION 'bangumi_season_id_positive does not match migration 0015';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conrelid = 'public.bangumi'::regclass
          AND conname = 'bangumi_metadata_object'
          AND pg_get_constraintdef(oid) =
              'CHECK ((jsonb_typeof(metadata) = ''object''::text))'
    ) THEN
        RAISE EXCEPTION 'bangumi_metadata_object does not match migration 0015';
    END IF;

    UPDATE _sqlx_migrations
    SET checksum = repository_checksum
    WHERE version = 15
      AND checksum = legacy_checksum;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'migration 15 changed during checksum repair';
    END IF;

    RAISE NOTICE 'migration 15 checksum repaired after schema verification';
END
$repair$;

COMMIT;
