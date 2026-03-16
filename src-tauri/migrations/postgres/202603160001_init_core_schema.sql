CREATE TABLE IF NOT EXISTS series (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    latest_excerpt TEXT NOT NULL,
    last_updated_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    archived_at TIMESTAMPTZ NULL,
    CONSTRAINT series_status_check
        CHECK (status IN ('active', 'silent', 'archived'))
);

CREATE TABLE IF NOT EXISTS commits (
    id TEXT PRIMARY KEY,
    series_id TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT commits_series_fk
        FOREIGN KEY (series_id) REFERENCES series (id)
);

CREATE TABLE IF NOT EXISTS consistency_alerts (
    id TEXT PRIMARY KEY,
    op_type TEXT NOT NULL,
    commit_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    resolved_at TIMESTAMPTZ NULL
);

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_series_last_updated_at
    ON series (last_updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_commits_series_created_at
    ON commits (series_id, created_at DESC);
