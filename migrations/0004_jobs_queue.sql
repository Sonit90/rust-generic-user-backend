-- Postgres-backed job queue.
-- Workers claim rows with `SELECT ... FOR UPDATE SKIP LOCKED`.

CREATE TYPE job_status AS ENUM ('queued', 'running', 'completed', 'failed', 'dead');

CREATE TABLE jobs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    kind          TEXT        NOT NULL,        -- 'file_purge', 'merge_run', ...
    payload       JSONB       NOT NULL,
    status        job_status  NOT NULL DEFAULT 'queued',
    attempts      INT         NOT NULL DEFAULT 0,
    max_attempts  INT         NOT NULL DEFAULT 5,
    run_at        TIMESTAMPTZ NOT NULL DEFAULT now(), -- "not before"
    locked_at     TIMESTAMPTZ,
    locked_by     TEXT,                        -- worker id
    last_error    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at   TIMESTAMPTZ
);

-- Workers query for queued jobs whose run_at is due.
CREATE INDEX jobs_due_idx ON jobs (run_at) WHERE status = 'queued';
CREATE INDEX jobs_kind_status_idx ON jobs (kind, status);

-- Helper: requeue jobs that were locked by a dead worker.
CREATE OR REPLACE FUNCTION requeue_stale_jobs(visibility_timeout INTERVAL)
RETURNS INTEGER LANGUAGE plpgsql AS $$
DECLARE
    n INTEGER;
BEGIN
    UPDATE jobs
       SET status   = 'queued',
           locked_at = NULL,
           locked_by = NULL
     WHERE status = 'running'
       AND locked_at < now() - visibility_timeout;
    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END;
$$;
