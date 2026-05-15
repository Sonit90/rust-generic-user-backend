-- Users, roles, permissions, OAuth identities, refresh tokens.

CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE EXTENSION IF NOT EXISTS citext;

-- Roles ----------------------------------------------------------------
CREATE TABLE roles (
    id          SMALLSERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT
);

INSERT INTO roles (name, description) VALUES
    ('admin',     'Full access; can manage users and permissions'),
    ('moderator', 'Can manage shared resources but not users'),
    ('user',      'Regular user');

-- Permissions ----------------------------------------------------------
CREATE TABLE permissions (
    id          SERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT
);

INSERT INTO permissions (name, description) VALUES
    ('users.read',          'View user list'),
    ('users.manage',        'Create/edit/disable users'),
    ('users.assign_roles',  'Assign roles to users'),
    ('files.upload',        'Upload price-list files'),
    ('files.read_own',      'Read own uploaded files'),
    ('files.read_all',      'Read any user''s files'),
    ('files.delete_own',    'Delete own files'),
    ('files.delete_any',    'Delete any file'),
    ('formats.manage_own',  'Create/edit own output formats'),
    ('formats.manage_any',  'Create/edit any output format'),
    ('mappings.manage_own', 'Create/edit own column mappings'),
    ('mappings.manage_any', 'Create/edit any column mapping'),
    ('jobs.run',            'Trigger merge/transform jobs');

CREATE TABLE role_permissions (
    role_id       SMALLINT NOT NULL REFERENCES roles(id)       ON DELETE CASCADE,
    permission_id INTEGER  NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_id)
);

-- Default role -> permissions mapping
INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r CROSS JOIN permissions p
WHERE r.name = 'admin';

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'moderator'
  AND p.name IN (
    'users.read',
    'files.upload', 'files.read_own', 'files.read_all',
    'files.delete_own', 'files.delete_any',
    'formats.manage_own', 'formats.manage_any',
    'mappings.manage_own', 'mappings.manage_any',
    'jobs.run'
  );

INSERT INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id FROM roles r, permissions p
WHERE r.name = 'user'
  AND p.name IN (
    'files.upload', 'files.read_own', 'files.delete_own',
    'formats.manage_own', 'mappings.manage_own',
    'jobs.run'
  );

-- Users ----------------------------------------------------------------
CREATE TABLE users (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email           CITEXT UNIQUE,
    password_hash   TEXT,                    -- NULL for OAuth-only users
    display_name    TEXT,
    role_id         SMALLINT NOT NULL REFERENCES roles(id),
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    email_verified  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX users_role_idx ON users(role_id);

-- OAuth identities (one user can link multiple providers) --------------
CREATE TABLE oauth_identities (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider    TEXT NOT NULL,        -- 'google', ...
    subject     TEXT NOT NULL,        -- provider-specific user id
    email       TEXT,
    raw_profile JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, subject)
);

CREATE INDEX oauth_identities_user_idx ON oauth_identities(user_id);

-- Refresh tokens (rotated; revocation by setting revoked_at) -----------
CREATE TABLE refresh_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE, -- store hash, not raw token
    issued_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ,
    user_agent  TEXT,
    ip          INET
);

CREATE INDEX refresh_tokens_user_idx ON refresh_tokens(user_id);
CREATE INDEX refresh_tokens_expires_idx ON refresh_tokens(expires_at);

-- Per-user permission overrides (optional, in addition to role) --------
CREATE TABLE user_permissions (
    user_id       UUID    NOT NULL REFERENCES users(id)       ON DELETE CASCADE,
    permission_id INTEGER NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    granted       BOOLEAN NOT NULL DEFAULT TRUE, -- false = explicit revoke
    PRIMARY KEY (user_id, permission_id)
);
