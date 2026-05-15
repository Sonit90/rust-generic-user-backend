-- Replace separate filter/transform columns with a unified ordered steps array.
-- Remove name (only filename remains as identifier).

ALTER TABLE output_formats
    DROP CONSTRAINT output_formats_owner_id_name_key;

ALTER TABLE output_formats
    DROP COLUMN name,
    DROP COLUMN file_filter,
    DROP COLUMN global_filter,
    DROP COLUMN transforms,
    ADD COLUMN steps JSONB NOT NULL DEFAULT '[]'::jsonb;
