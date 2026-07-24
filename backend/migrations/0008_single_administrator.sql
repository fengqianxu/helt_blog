-- This is a personal blog with exactly one administrator account.
-- Authentication is binary (administrator or anonymous); there is no RBAC.
CREATE UNIQUE INDEX admin_users_singleton
    ON admin_users ((true));
