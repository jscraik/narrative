//! Contribution stats computation

use super::line_attribution::LineAttributionCommitRow;
use super::models::ContributionStats;
use super::source_lens::LineMeta;
use super::{
    line_attribution::fetch_line_attributions_for_commit,
    utils::fetch_repo_root,
};
use crate::linking::SessionExcerpt;
use crate::attribution::models::AttributionError;
use git2::Repository;
use std::collections::HashMap;

/// Database row for contribution stats
#[derive(sqlx::FromRow)]
pub(super) struct ContributionStatsRow {
    pub human_lines: i32,
    pub ai_agent_lines: i32,
    pub ai_assist_lines: i32,
    pub collaborative_lines: i32,
    pub total_lines: i32,
    pub ai_percentage: i32,
    pub tool: Option<String>,
    pub model: Option<String>,
}

impl ContributionStatsRow {
    pub(super) fn into_stats(self, tool_breakdown: Option<Vec<super::models::ToolStats>>) -> ContributionStats {
        ContributionStats {
            human_lines: self.human_lines as u32,
            ai_agent_lines: self.ai_agent_lines as u32,
            ai_assist_lines: self.ai_assist_lines as u32,
            collaborative_lines: self.collaborative_lines as u32,
            total_lines: self.total_lines as u32,
            ai_percentage: self.ai_percentage as f32,
            tool_breakdown,
            primary_tool: self.tool,
            model: self.model,
        }
    }
}

#[derive(sqlx::FromRow)]
pub(super) struct ToolStatsRow {
    pub tool: String,
    pub model: Option<String>,
    pub line_count: i32,
}

#[derive(sqlx::FromRow)]
pub(super) struct LinkedSessionRow {
    pub session_id: String,
    pub tool: String,
    pub model: Option<String>,
    pub files: Option<String>,
}

/// Fetch cached stats from database
pub async fn fetch_cached_stats(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Option<ContributionStats> {
    let row = sqlx::query_as::<_, ContributionStatsRow>(
        r#"
        SELECT human_lines, ai_agent_lines, ai_assist_lines, collaborative_lines,
               total_lines, ai_percentage, tool, model
        FROM commit_contribution_stats
        WHERE repo_id = ? AND commit_sha = ?
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .fetch_optional(db)
    .await
    .ok()?;

    let breakdown = fetch_tool_breakdown(db, repo_id, commit_sha)
        .await
        .ok()
        .flatten();
    row.map(|r| r.into_stats(breakdown))
}

/// Fetch linked session for a commit
pub async fn fetch_linked_session(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<SessionExcerpt, AttributionError> {
    use crate::models::SessionLink;

    // Get session link
    let link: SessionLink = sqlx::query_as(
        r#"
        SELECT * FROM session_links
        WHERE repo_id = ? AND commit_sha = ?
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .fetch_one(db)
    .await
    .map_err(|_| AttributionError::SessionNotFound)?;

    // Get session data
    let session_json: String = sqlx::query_scalar(
        r#"
        SELECT raw_json FROM sessions WHERE id = ?
        "#,
    )
    .bind(&link.session_id)
    .fetch_one(db)
    .await
    .map_err(|e| AttributionError::DatabaseError(e.to_string()))?;

    // Parse session
    let session: SessionExcerpt = serde_json::from_str(&session_json)
        .map_err(|e| AttributionError::DatabaseError(format!("Failed to parse session: {}", e)))?;

    Ok(session)
}

/// Fetch files changed in a commit
pub async fn fetch_commit_files(
    _db: &sqlx::SqlitePool,
    _repo_id: i64,
    _commit_sha: &str,
) -> Result<Vec<String>, AttributionError> {
    // TODO: Get files from git or from stored commit data
    // For now, return empty (stats computation will use session files only)
    Ok(vec![])
}

/// Fetch tool breakdown for a commit
pub async fn fetch_tool_breakdown(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Option<Vec<super::models::ToolStats>>, String> {
    let rows = sqlx::query_as::<_, ToolStatsRow>(
        r#"
        SELECT tool, model, line_count
        FROM commit_tool_stats
        WHERE repo_id = ? AND commit_sha = ?
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    if rows.is_empty() {
        return Ok(None);
    }

    let stats = rows
        .into_iter()
        .map(|row| super::models::ToolStats {
            tool: row.tool,
            model: row.model,
            line_count: row.line_count.max(0) as u32,
        })
        .collect::<Vec<_>>();

    Ok(Some(stats))
}

/// Compute contribution stats from line attributions
pub async fn compute_contribution_from_attributions(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Option<ContributionStats>, String> {
    use super::source_lens::{build_line_meta, LineAttributionRow};
    use std::collections::HashMap;

    let rows = fetch_line_attributions_for_commit(db, repo_id, commit_sha).await?;
    if rows.is_empty() {
        return Ok(None);
    }

    let repo_root = fetch_repo_root(db, repo_id).await?;
    let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;

    let mut by_file: HashMap<String, Vec<LineAttributionCommitRow>> = HashMap::new();
    for row in rows {
        by_file.entry(row.file_path.clone()).or_default().push(row);
    }

    let mut stats = ContributionStats::default();
    let mut tool_counts: HashMap<(String, Option<String>), u32> = HashMap::new();

    for (file_path, attrs) in by_file {
        let file_lines = match super::source_lens::load_file_lines(&repo, commit_sha, &file_path) {
            Ok(lines) => lines,
            Err(_) => continue,
        };
        if file_lines.is_empty() {
            continue;
        }

        let file_attrs = attrs
            .into_iter()
            .map(|row| LineAttributionRow {
                start_line: row.start_line,
                end_line: row.end_line,
                session_id: row.session_id,
                author_type: row.author_type,
                ai_percentage: row.ai_percentage,
                tool: row.tool,
                model: row.model,
                trace_available: 0,
            })
            .collect::<Vec<_>>();

        let line_meta = build_line_meta(file_lines.len(), &file_attrs);

        for meta in line_meta {
            match meta.author_type.as_str() {
                "ai_agent" => {
                    stats.ai_agent_lines += 1;
                    increment_tool_count(&mut tool_counts, &meta);
                }
                "ai_tab" => {
                    stats.ai_assist_lines += 1;
                    increment_tool_count(&mut tool_counts, &meta);
                }
                "mixed" => {
                    stats.collaborative_lines += 1;
                }
                _ => {
                    stats.human_lines += 1;
                }
            }
        }
    }

    stats.total_lines = stats.human_lines
        + stats.ai_agent_lines
        + stats.ai_assist_lines
        + stats.collaborative_lines;

    if stats.total_lines > 0 {
        let ai_total = stats.ai_agent_lines + stats.ai_assist_lines + stats.collaborative_lines;
        stats.ai_percentage = (ai_total as f32 / stats.total_lines as f32) * 100.0;
    }

    if !tool_counts.is_empty() {
        let mut breakdown = tool_counts
            .into_iter()
            .map(|((tool, model), count)| super::models::ToolStats {
                tool,
                model,
                line_count: count,
            })
            .collect::<Vec<_>>();
        breakdown.sort_by(|a, b| b.line_count.cmp(&a.line_count));
        stats.primary_tool = breakdown.first().map(|b| b.tool.clone());
        stats.model = breakdown.first().and_then(|b| b.model.clone());
        stats.tool_breakdown = Some(breakdown);
    }

    Ok(Some(stats))
}

fn increment_tool_count(counts: &mut HashMap<(String, Option<String>), u32>, meta: &LineMeta) {
    let tool = meta.tool.clone().unwrap_or_else(|| "unknown".to_string());
    let key = (tool, meta.model.clone());
    *counts.entry(key).or_insert(0) += 1;
}
