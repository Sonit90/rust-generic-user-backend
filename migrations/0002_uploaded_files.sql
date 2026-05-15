-- Uploaded price-list files. Bytes live in object storage; we keep metadata here.

CREATE TYPE file_kind AS ENUM ('csv', 'xls', 'xlsx');

CREATE TABLE uploaded_files (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    original_name TEXT NOT NULL,
    storage_key   TEXT NOT NULL UNIQUE, -- path in object storage
    kind          file_kind NOT NULL,
    mime_type     TEXT,
    size_bytes    BIGINT NOT NULL,
    sha256        TEXT,                 -- hex digest for de-dup / integrity
    expires_at    TIMESTAMPTZ NOT NULL,
    purged_at     TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX uploaded_files_owner_idx     ON uploaded_files(owner_id);
CREATE INDEX uploaded_files_expires_idx   ON uploaded_files(expires_at) WHERE purged_at IS NULL;
CREATE INDEX uploaded_files_sha256_idx    ON uploaded_files(sha256);

-- Generated/output files produced by merge jobs. Same retention rules.
CREATE TABLE output_files (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    storage_key   TEXT NOT NULL UNIQUE,
    kind          file_kind NOT NULL,
    size_bytes    BIGINT NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL,
    purged_at     TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX output_files_owner_idx   ON output_files(owner_id);
CREATE INDEX output_files_expires_idx ON output_files(expires_at) WHERE purged_at IS NULL;
