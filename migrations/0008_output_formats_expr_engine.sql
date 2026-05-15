-- Replace the static GlobalTransform enum with a JSON expression engine.
-- Rename output_kind → output_extension, add filename, add three expression columns.

ALTER TABLE output_formats
    RENAME COLUMN output_kind TO output_extension;

ALTER TABLE output_formats
    ADD COLUMN filename      TEXT,
    ADD COLUMN file_filter   JSONB,
    ADD COLUMN global_filter JSONB,
    ADD COLUMN transforms    JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE output_formats
    DROP COLUMN global_transforms;
