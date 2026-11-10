CREATE TABLE merge_authorizations (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    assignment_id TEXT NOT NULL,
    head_commit TEXT NOT NULL,
    base_commit TEXT NOT NULL,
    target_branch TEXT NOT NULL,
    source_branch TEXT NOT NULL,
    authorized_by TEXT NOT NULL,
    expires_at_unix INTEGER NOT NULL,
    consumed INTEGER NOT NULL DEFAULT 0 CHECK (consumed IN (0, 1)),
    consumed_at_unix INTEGER
);
