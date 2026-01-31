//! Data models for attribution tracking

use super::coverage::AttributionCoverageSummary;
use serde::{Deserialize, Serialize};

/// Statistics about a commit's contributions
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContributionStats {
    /// Lines written by human
    pub human_lines: u32,
    /// Lines from AI agent
    pub ai_agent_lines: u32,
    /// Lines from AI assist (completions)
    pub ai_assist_lines: u32,
    /// Lines with both AI and human contribution
    pub collaborative_lines: u32,
    /// Total lines in commit
    pub total_lines: u32,
    /// Overall AI percentage (0-100)
    pub ai_percentage: f32,
    /// Tool-specific breakdown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_breakdown: Option<Vec<ToolStats>>,
    /// Primary tool used (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_tool: Option<String>,
    /// Model used (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl ContributionStats {
    /// Create stats for a purely human commit
    pub fn human_only(total_lines: u32) -> Self {
        Self {
            human_lines: total_lines,
            total_lines,
            ai_percentage: 0.0,
            ..Default::default()
        }
    }
}

/// Statistics for a specific tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    pub tool: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub line_count: u32,
}

/// A line with its source attribution (UI-ready)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLine {
    pub line_number: u32,
    pub content: String,
    pub author_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_percentage: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub trace_available: bool,
}

/// Request for source lens data
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLensRequest {
    pub repo_id: i64,
    pub commit_sha: String,
    pub file_path: String,
    pub offset: u32,
    pub limit: u32,
}

/// Response for source lens data
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLensPage {
    pub lines: Vec<SourceLine>,
    pub total_lines: u32,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionNoteSummary {
    pub commit_sha: String,
    pub has_note: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    pub metadata_available: bool,
    pub metadata_cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<AttributionCoverageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence_source: Option<String>,
}

/// Errors that can occur in attribution operations
#[derive(Debug, Clone)]
pub enum AttributionError {
    DatabaseError(String),
    SessionNotFound,
}

impl std::fmt::Display for AttributionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AttributionError::DatabaseError(e) => write!(f, "Database error: {}", e),
            AttributionError::SessionNotFound => write!(f, "Session not found"),
        }
    }
}

impl std::error::Error for AttributionError {}
