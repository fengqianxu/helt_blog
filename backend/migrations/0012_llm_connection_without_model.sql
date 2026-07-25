-- Credentials are saved after their /models endpoint is verified. Model
-- selection belongs to each use case, not to the credential itself.
ALTER TABLE llm_connections
    DROP CONSTRAINT IF EXISTS llm_connections_model_not_blank;

ALTER TABLE llm_connections
    ALTER COLUMN model SET DEFAULT '';
