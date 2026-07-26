ALTER TABLE bangumi
    ADD COLUMN season_id BIGINT,
    ADD COLUMN metadata JSONB NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE bangumi
    ADD CONSTRAINT bangumi_season_id_positive
        CHECK (season_id IS NULL OR season_id > 0),
    ADD CONSTRAINT bangumi_metadata_object
        CHECK (jsonb_typeof(metadata) = 'object');
