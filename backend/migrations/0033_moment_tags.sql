CREATE TABLE moment_tags (
    moment_id BIGINT NOT NULL REFERENCES moments(id) ON DELETE CASCADE,
    tag_id BIGINT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (moment_id, tag_id)
);

CREATE INDEX idx_moment_tags_tag ON moment_tags (tag_id, moment_id);
