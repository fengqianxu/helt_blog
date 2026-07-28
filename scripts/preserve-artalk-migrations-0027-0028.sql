\set ON_ERROR_STOP on

-- Retire the already-published destructive Artalk reset migrations without
-- rewriting them. Run this only after taking the external pg_dump documented in
-- DEPLOY.md. The transaction also creates an in-database snapshot for quick
-- verification/recovery, then records the exact immutable migration checksums.
BEGIN;
LOCK TABLE _sqlx_migrations IN ACCESS EXCLUSIVE MODE;

DO $$
DECLARE
    table_name TEXT;
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM _sqlx_migrations WHERE version = 26 AND success
    ) THEN
        RAISE EXCEPTION 'migration 26 must be applied before preserving 27/28';
    END IF;
    IF EXISTS (
        SELECT 1 FROM _sqlx_migrations
        WHERE version IN (27, 28) AND NOT success
    ) THEN
        RAISE EXCEPTION 'a failed migration 27/28 record requires manual investigation';
    END IF;

    IF EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'artalk_preservation_0027_0028') THEN
        RAISE EXCEPTION 'snapshot schema artalk_preservation_0027_0028 already exists';
    END IF;
    CREATE SCHEMA artalk_preservation_0027_0028;

    FOREACH table_name IN ARRAY ARRAY[
        'artalk_notifies',
        'artalk_votes',
        'artalk_comments',
        'artalk_auth_identities',
        'artalk_user_email_verifies',
        'artalk_pages',
        'artalk_users'
    ] LOOP
        IF to_regclass(table_name) IS NOT NULL THEN
            EXECUTE format(
                'CREATE TABLE artalk_preservation_0027_0028.%I AS TABLE %I',
                table_name,
                table_name
            );
        END IF;
    END LOOP;
END
$$;

INSERT INTO _sqlx_migrations (
    version, description, installed_on, success, checksum, execution_time
)
VALUES
    (
        27,
        'clear artalk comment data',
        now(),
        TRUE,
        decode('3cd6919689d1d0baf35d04b17bc27aed80e96281290a7b2b00326930deab898454de98accf7800c5db7eb188c6a6dd6f', 'hex'),
        0
    ),
    (
        28,
        'clear artalk comment metadata',
        now(),
        TRUE,
        decode('436122486b9f77c92dd40ca50b98f341a04c522f319ed805828d8d64f183dc8d9c1d9357ee4b90e7d0a0676ec9431281', 'hex'),
        0
    )
ON CONFLICT (version) DO NOTHING;

COMMIT;

SELECT version, description, success
FROM _sqlx_migrations
WHERE version IN (27, 28)
ORDER BY version;
