-- Artalk is the only persistent comment system. The legacy table and its
-- application-owned moderation settings must not remain as a second source.
DROP TABLE IF EXISTS comments;

UPDATE llm_settings
SET use_cases = use_cases - 'comment_review'
WHERE id = 1;

UPDATE site_settings
SET settings = (settings #- '{comments}') #- '{features,comments}'
WHERE id = 1;
