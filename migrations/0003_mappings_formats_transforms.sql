-- Per-file column mappings, user-defined output formats, transformations.

-- A "column mapping" is the user's markup of an uploaded file:
-- which sheet to read, which row contains the headers, and what each column
-- represents in the canonical schema (sku, name, brand, price, qty, ...).
CREATE TABLE column_mappings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    file_id         UUID NOT NULL REFERENCES uploaded_files(id) ON DELETE CASCADE,
    sheet_name      TEXT,            -- NULL for CSV
    header_row      INT  NOT NULL DEFAULT 1,
    data_start_row  INT  NOT NULL DEFAULT 2,
    -- Array of column-mapping entries:
    -- [{ "source_index": 0, "source_header": "Артикул", "canonical": "sku",
    --    "data_type": "string", "transformations": [...] }, ...]
    columns         JSONB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX column_mappings_owner_idx ON column_mappings(owner_id);
CREATE INDEX column_mappings_file_idx  ON column_mappings(file_id);

-- Reusable output format. Defines columns, ordering, and global transforms.
CREATE TABLE output_formats (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    description     TEXT,
    -- Array of output-column specs:
    -- [{ "name": "SKU",   "canonical": "sku",   "extra": false },
    --  { "name": "Price", "canonical": "price", "extra": false },
    --  { "name": "Markup","canonical": null,    "extra": true,
    --    "default_value": "" }, ...]
    columns         JSONB NOT NULL,
    -- Transforms applied to the merged dataset as a whole.
    -- [{ "kind": "increase_percent", "target": "price", "value": 20 },
    --  { "kind": "filter",           "target": "qty",   "op": "gt", "value": 0 }]
    global_transforms JSONB NOT NULL DEFAULT '[]'::jsonb,
    output_kind     file_kind NOT NULL DEFAULT 'xlsx',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (owner_id, name)
);

CREATE INDEX output_formats_owner_idx ON output_formats(owner_id);

-- A merge run: which mappings are combined into which output format.
CREATE TYPE merge_status AS ENUM ('queued', 'running', 'completed', 'failed', 'cancelled');

CREATE TABLE merge_runs (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    output_format_id  UUID NOT NULL REFERENCES output_formats(id) ON DELETE RESTRICT,
    -- Array of input column-mapping ids participating in this run.
    input_mapping_ids UUID[] NOT NULL,
    status            merge_status NOT NULL DEFAULT 'queued',
    output_file_id    UUID REFERENCES output_files(id) ON DELETE SET NULL,
    error_message     TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at        TIMESTAMPTZ,
    finished_at       TIMESTAMPTZ
);

CREATE INDEX merge_runs_owner_idx  ON merge_runs(owner_id);
CREATE INDEX merge_runs_status_idx ON merge_runs(status);
