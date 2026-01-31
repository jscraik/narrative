-- Migration: Attribution note metadata + prompt cache + prefs

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS attribution_note_meta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    commit_sha TEXT NOT NULL,
    note_ref TEXT NOT NULL,
    note_hash TEXT NOT NULL,
    schema_version TEXT,
    metadata_available INTEGER NOT NULL DEFAULT 0 CHECK(metadata_available IN (0, 1)),
    metadata_cached INTEGER NOT NULL DEFAULT 0 CHECK(metadata_cached IN (0, 1)),
    prompt_count INTEGER DEFAULT 0,
    imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(repo_id, commit_sha, note_ref),
    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_attr_note_meta_commit ON attribution_note_meta(repo_id, commit_sha);
CREATE INDEX IF NOT EXISTS idx_attr_note_meta_hash ON attribution_note_meta(note_hash);

CREATE TABLE IF NOT EXISTS attribution_prompt_meta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL,
    prompt_id TEXT NOT NULL,
    commit_sha TEXT NOT NULL,
    tool TEXT,
    model TEXT,
    human_author TEXT,
    summary TEXT,
    total_additions INTEGER,
    total_deletions INTEGER,
    accepted_lines INTEGER,
    overridden_lines INTEGER,
    prompt_json TEXT,
    contains_messages INTEGER NOT NULL DEFAULT 0 CHECK(contains_messages IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    UNIQUE(repo_id, commit_sha, prompt_id),
    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_attr_prompt_meta_commit ON attribution_prompt_meta(repo_id, commit_sha);
CREATE INDEX IF NOT EXISTS idx_attr_prompt_meta_tool ON attribution_prompt_meta(repo_id, tool);

CREATE TABLE IF NOT EXISTS attribution_prefs (
    repo_id INTEGER PRIMARY KEY,
    cache_prompt_metadata INTEGER NOT NULL DEFAULT 0 CHECK(cache_prompt_metadata IN (0, 1)),
    store_prompt_text INTEGER NOT NULL DEFAULT 0 CHECK(store_prompt_text IN (0, 1)),
    show_line_overlays INTEGER NOT NULL DEFAULT 1 CHECK(show_line_overlays IN (0, 1)),
    retention_days INTEGER,
    last_purged_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,

    FOREIGN KEY (repo_id) REFERENCES repos(id) ON DELETE CASCADE
);
