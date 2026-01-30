-- Migration: Commit rewrite keys
-- Stores stable rewrite keys for attribution recovery across history rewrites

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS commit_rewrite_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    commit_sha TEXT NOT NULL,
    rewrite_key TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(repo_id, commit_sha),
    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_rewrite_keys_lookup ON commit_rewrite_keys(repo_id, rewrite_key);
CREATE INDEX IF NOT EXISTS idx_rewrite_keys_commit ON commit_rewrite_keys(repo_id, commit_sha);
