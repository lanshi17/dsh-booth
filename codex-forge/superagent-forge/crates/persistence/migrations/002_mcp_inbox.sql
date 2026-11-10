CREATE TABLE mcp_inbox (
    message_id TEXT PRIMARY KEY,
    response_json TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL
);
