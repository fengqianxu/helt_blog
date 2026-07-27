-- Complete the friend-link application and review workflow.
--
-- Contact details are visible to administrators only. The request fingerprint
-- is a one-way hash used to enforce the public submission rate limit without
-- retaining a visitor IP address.
ALTER TABLE friends
    ADD COLUMN contact_email TEXT NOT NULL DEFAULT '',
    ADD COLUMN submission_ip_hash TEXT,
    ADD COLUMN reviewed_at TIMESTAMPTZ,
    ADD CONSTRAINT friends_contact_email_length
        CHECK (char_length(contact_email) <= 254),
    ADD CONSTRAINT friends_submission_ip_hash_format
        CHECK (
            submission_ip_hash IS NULL
            OR submission_ip_hash ~ '^[0-9a-f]{64}$'
        );

CREATE INDEX idx_friends_submission_rate
    ON friends (submission_ip_hash, created_at DESC)
    WHERE submission_ip_hash IS NOT NULL;

CREATE INDEX idx_friends_admin_review
    ON friends (status, created_at DESC, id DESC);
