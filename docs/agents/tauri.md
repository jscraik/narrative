# tauri and data locations

## tauri v2 permissions

Define tauri v2 permissions in:

- src-tauri/capabilities/default.json

Enabled capabilities (per README):

- dialog:allow-open
- shell:allow-execute (scoped to git)
- sql:default + sql:allow-execute

## repo writes

When you open a repo, the app writes committable metadata under:

- .narrative/meta/repo.json
- .narrative/meta/branches/<branch>.json
- .narrative/meta/commits/<sha>.json
- .narrative/meta/commits/<sha>.files.json (lazy)

## Sessions

The importer writes sessions to:

- .narrative/sessions/imported/*

The importer performs basic secret redaction before writing.

## Local cache

SQLite cache (not intended for commit):

- sqlite:narrative.db
- Migration: src-tauri/migrations/001_init.sql
