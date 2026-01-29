-- Migration 003: Add Agent Trace tables
-- Purpose: Store Agent Trace records + file/range attribution
-- Dependencies: Migration 001 (repos, commits) + 002 (session_links)

-- UP: Create trace tables
CREATE TABLE IF NOT EXISTS trace_records (
  id TEXT PRIMARY KEY,
  repo_id INTEGER NOT NULL,
  version TEXT NOT NULL,
  timestamp TEXT NOT NULL,
  vcs_type TEXT NOT NULL,
  revision TEXT NOT NULL,
  tool_name TEXT,
  tool_version TEXT,
  metadata_json TEXT,
  created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  FOREIGN KEY(repo_id) REFERENCES repos(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS trace_files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  record_id TEXT NOT NULL,
  path TEXT NOT NULL,
  FOREIGN KEY(record_id) REFERENCES trace_records(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS trace_conversations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  file_id INTEGER NOT NULL,
  url TEXT,
  contributor_type TEXT NOT NULL,
  model_id TEXT,
  FOREIGN KEY(file_id) REFERENCES trace_files(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS trace_ranges (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL,
  start_line INTEGER NOT NULL,
  end_line INTEGER NOT NULL,
  content_hash TEXT,
  contributor_type TEXT NOT NULL,
  model_id TEXT,
  FOREIGN KEY(conversation_id) REFERENCES trace_conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_trace_records_repo_revision ON trace_records(repo_id, revision);
CREATE INDEX IF NOT EXISTS idx_trace_files_record ON trace_files(record_id);
CREATE INDEX IF NOT EXISTS idx_trace_files_path ON trace_files(record_id, path);
CREATE INDEX IF NOT EXISTS idx_trace_ranges_conv ON trace_ranges(conversation_id);

-- DOWN: Rollback migration
DROP INDEX IF EXISTS idx_trace_ranges_conv;
DROP INDEX IF EXISTS idx_trace_files_path;
DROP INDEX IF EXISTS idx_trace_files_record;
DROP INDEX IF EXISTS idx_trace_records_repo_revision;
DROP TABLE IF EXISTS trace_ranges;
DROP TABLE IF EXISTS trace_conversations;
DROP TABLE IF EXISTS trace_files;
DROP TABLE IF EXISTS trace_records;
