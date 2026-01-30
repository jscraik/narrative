//! Attribution tracking for AI-generated code
//!
//! This module provides:
//! - Contribution statistics per commit (MVP)
//! - Line-level attribution (Phase 2)
//! - Source lens for viewing attribution inline (Phase 2)
//!
//! Module organization:
//! - `commands.rs` - Tauri command wrappers (thin layer to external modules)
//! - `models.rs` - Public-facing types and data structures
//! - `notes.rs` - Git note parsing and formatting
//! - `session_stats.rs` - Session contribution computation
//! - `stats.rs` - Contribution stats computation (database queries, aggregation)
//! - `source_lens.rs` - Line attribution display with pagination
//! - `notes_io.rs` - Git note import/export commands
//! - `line_attribution.rs` - Line attribution storage and retrieval
//! - `git_utils.rs` - Git operations (diff, patch-id, file listing)
//! - `utils.rs` - Shared utilities (repo root fetching, session metadata)

pub mod commands;
pub mod git_utils;
pub mod line_attribution;
pub mod models;
pub mod notes;
pub mod notes_io;
pub mod session_stats;
pub mod source_lens;
pub mod stats;
pub mod utils;
