-- Applied by forge-persistence Turso backend. Each connection enables foreign_keys.
-- Writer transactions use version CAS on tasks.version.
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at_unix INTEGER NOT NULL
);
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    version INTEGER NOT NULL CHECK (version >= 1),
    snapshot_json TEXT NOT NULL
);
CREATE TABLE assignments (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    worker_id TEXT NOT NULL,
    directory TEXT NOT NULL UNIQUE,
    branch TEXT NOT NULL UNIQUE
);
CREATE TABLE reports (
    id TEXT PRIMARY KEY,
    assignment_id TEXT NOT NULL REFERENCES assignments(id),
    head_commit TEXT NOT NULL,
    report_json TEXT NOT NULL
);
CREATE TABLE reviews (
    id TEXT PRIMARY KEY,
    report_id TEXT NOT NULL REFERENCES reports(id),
    reviewer_id TEXT NOT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('approve', 'reject')),
    evidence_json TEXT NOT NULL
);
CREATE TABLE outbox (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    event_json TEXT NOT NULL,
    delivered INTEGER NOT NULL DEFAULT 0 CHECK (delivered IN (0, 1))
);
