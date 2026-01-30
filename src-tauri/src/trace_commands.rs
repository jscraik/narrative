//! Tauri commands for Agent Trace operations
//!
//! Provides commands to query Agent Trace data using the sqlx pool,
//! which ensures we see the migrated database schema.

use serde::Serialize;
use sqlx::Row;
use tauri::State;

use crate::DbState;

/// Trace summary for a commit
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceCommitSummary {
    pub commit_sha: String,
    pub ai_lines: i64,
    pub human_lines: i64,
    pub mixed_lines: i64,
    pub unknown_lines: i64,
    pub ai_percent: i64,
    pub model_ids: Vec<String>,
    pub tool_names: Vec<String>,
}

/// File-level trace summary
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceFileSummary {
    pub path: String,
    pub ai_lines: i64,
    pub human_lines: i64,
    pub mixed_lines: i64,
    pub unknown_lines: i64,
    pub ai_percent: i64,
}

/// Complete trace summary result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummaryResult {
    pub commit: TraceCommitSummary,
    pub files: std::collections::HashMap<String, TraceFileSummary>,
    pub totals: Totals,
}

/// Totals for conversations and ranges
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub conversations: i64,
    pub ranges: i64,
}

/// Get trace summary for a specific commit
#[tauri::command(rename_all = "camelCase")]
pub async fn get_trace_summary_for_commit(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Option<TraceSummaryResult>, String> {
    let pool = &*db.0; // Get &SqlitePool from Arc<SqlitePool>

    // Query all trace ranges for this commit
    let rows = sqlx::query(
        "SELECT tf.path, tr.start_line, tr.end_line, tr.contributor_type, tr.model_id
         FROM trace_records r
         JOIN trace_files tf ON tf.record_id = r.id
         JOIN trace_conversations tc ON tc.file_id = tf.id
         JOIN trace_ranges tr ON tr.conversation_id = tc.id
         WHERE r.repo_id = $1 AND r.revision = $2"
    )
    .bind(repo_id)
    .bind(&commit_sha)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Database query failed: {}", e))?;

    if rows.is_empty() {
        return Ok(None);
    }

    let mut file_map: std::collections::HashMap<String, TraceFileSummary> = std::collections::HashMap::new();
    let mut model_ids = std::collections::HashSet::new();
    let mut ai_lines = 0i64;
    let mut human_lines = 0i64;
    let mut mixed_lines = 0i64;
    let mut unknown_lines = 0i64;

    for row in rows {
        let path: String = row.get("path");
        let start_line: i64 = row.get("start_line");
        let end_line: i64 = row.get("end_line");
        let contributor_type: String = row.get("contributor_type");
        let model_id: Option<String> = row.get("model_id");

        let count = end_line - start_line + 1;

        let entry = file_map.entry(path.clone()).or_insert_with(|| TraceFileSummary {
            path: path.clone(),
            ai_lines: 0,
            human_lines: 0,
            mixed_lines: 0,
            unknown_lines: 0,
            ai_percent: 0,
        });

        match contributor_type.as_str() {
            "ai" => {
                ai_lines += count;
                entry.ai_lines += count;
            }
            "human" => {
                human_lines += count;
                entry.human_lines += count;
            }
            "mixed" => {
                mixed_lines += count;
                entry.mixed_lines += count;
            }
            _ => {
                unknown_lines += count;
                entry.unknown_lines += count;
            }
        }

        if let Some(mid) = model_id {
            model_ids.insert(mid);
        }
    }

    // Calculate AI percentages for each file
    for file in file_map.values_mut() {
        let total = file.ai_lines + file.human_lines + file.mixed_lines + file.unknown_lines;
        file.ai_percent = if total > 0 {
            file.ai_lines * 100 / total
        } else {
            0
        };
    }

    let total_lines = ai_lines + human_lines + mixed_lines + unknown_lines;
    let ai_percent = if total_lines > 0 {
        ai_lines * 100 / total_lines
    } else {
        0
    };

    // Get distinct tool names for this commit
    let tool_rows = sqlx::query(
        "SELECT DISTINCT tool_name
         FROM trace_records
         WHERE repo_id = $1 AND revision = $2 AND tool_name IS NOT NULL"
    )
    .bind(repo_id)
    .bind(&commit_sha)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to get tool names: {}", e))?;

    let tool_names: Vec<String> = tool_rows
        .iter()
        .map(|row| row.get::<String, _>("tool_name"))
        .collect();

    // Get totals
    let totals_row = sqlx::query(
        "SELECT COUNT(DISTINCT tc.id) as conversations, COUNT(tr.id) as ranges
         FROM trace_records r
         JOIN trace_files tf ON tf.record_id = r.id
         JOIN trace_conversations tc ON tc.file_id = tf.id
         JOIN trace_ranges tr ON tr.conversation_id = tc.id
         WHERE r.repo_id = $1 AND r.revision = $2"
    )
    .bind(repo_id)
    .bind(&commit_sha)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to get totals: {}", e))?;

    let conversations: i64 = totals_row.get("conversations");
    let ranges: i64 = totals_row.get("ranges");

    Ok(Some(TraceSummaryResult {
        commit: TraceCommitSummary {
            commit_sha: commit_sha.to_string(),
            ai_lines,
            human_lines,
            mixed_lines,
            unknown_lines,
            ai_percent,
            model_ids: model_ids.into_iter().collect(),
            tool_names,
        },
        files: file_map,
        totals: Totals {
            conversations,
            ranges,
        },
    }))
}

/// Get trace summaries for multiple commits
#[tauri::command(rename_all = "camelCase")]
pub async fn get_trace_summaries_for_commits(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_shas: Vec<String>,
) -> Result<std::collections::HashMap<String, TraceSummaryResult>, String> {
    let mut results = std::collections::HashMap::new();

    for sha in commit_shas {
        if let Ok(Some(summary)) = get_trace_summary_for_commit(db.clone(), repo_id, &sha).await {
            results.insert(sha, summary);
        }
    }

    Ok(results)
}

/// Trace range for a file
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceRange {
    pub start_line: i64,
    pub end_line: i64,
    pub content_hash: Option<String>,
    pub contributor: TraceContributor,
}

/// Contributor information
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceContributor {
    pub contributor_type: String,
    pub model_id: Option<String>,
}

/// Get trace ranges for a specific commit and file
#[tauri::command(rename_all = "camelCase")]
pub async fn get_trace_ranges_for_commit_file(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_sha: String,
    file_path: String,
) -> Result<Vec<TraceRange>, String> {
    let pool = &*db.0; // Get &SqlitePool from Arc<SqlitePool>

    let rows = sqlx::query(
        "SELECT tr.start_line, tr.end_line, tr.content_hash, tr.contributor_type, tr.model_id
         FROM trace_records r
         JOIN trace_files tf ON tf.record_id = r.id
         JOIN trace_conversations tc ON tc.file_id = tf.id
         JOIN trace_ranges tr ON tr.conversation_id = tc.id
         WHERE r.repo_id = $1 AND r.revision = $2 AND tf.path = $3
         ORDER BY tr.start_line ASC"
    )
    .bind(repo_id)
    .bind(&commit_sha)
    .bind(&file_path)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Database query failed: {}", e))?;

    let ranges = rows
        .iter()
        .map(|row| TraceRange {
            start_line: row.get("start_line"),
            end_line: row.get("end_line"),
            content_hash: row.get::<Option<String>, _>("content_hash"),
            contributor: TraceContributor {
                contributor_type: row.get("contributor_type"),
                model_id: row.get::<Option<String>, _>("model_id"),
            },
        })
        .collect();

    Ok(ranges)
}
