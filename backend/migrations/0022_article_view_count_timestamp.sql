-- Reading an article increments view_count, but that operational counter must
-- not look like an editorial change. Otherwise every public view invalidates
-- the editor's optimistic-lock timestamp and reorders "recently updated" rows.
CREATE OR REPLACE FUNCTION set_article_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    IF (to_jsonb(NEW) - 'view_count' - 'updated_at')
       IS DISTINCT FROM
       (to_jsonb(OLD) - 'view_count' - 'updated_at') THEN
        NEW.updated_at = now();
    ELSE
        NEW.updated_at = OLD.updated_at;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER trg_articles_updated_at ON articles;

CREATE TRIGGER trg_articles_updated_at
BEFORE UPDATE ON articles
FOR EACH ROW EXECUTE FUNCTION set_article_updated_at();
