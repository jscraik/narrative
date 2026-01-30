//! Tauri commands for attribution operations
//!
//! This file provides thin Tauri command wrappers that delegate to
//! the appropriate internal modules. Each command is a small facade
//! over the actual implementation logic.

use super::models::ContributionStats;
use super::notes_io::{
    AttributionNoteBatchSummary, AttributionNoteExportSummary, AttributionNoteImportSummary,
};
use super::session_stats::compute_human_contribution;
use super::stats::{compute_contribution_from_attributions, fetch_cached_stats, fetch_linked_session};
use crate::DbState;
use tauri::State;

/// Get contribution stats for a commit
///
/// Returns cached stats if available, otherwise computes from linked session.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_commit_contribution_stats(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_sha: String,
) -> Result<ContributionStats, String> {
    use super::line_attribution::ensure_line_attributions_for_commit;
    use super::session_stats::store_contribution_stats;

    let _ = ensure_line_attributions_for_commit(&db.0, repo_id, &commit_sha).await;

    // Try to get cached stats first
    if let Some(stats) = fetch_cached_stats(&db.0, repo_id, &commit_sha).await {
        return Ok(stats);
    }

    // Prefer line-level attribution if available
    if let Ok(Some(stats)) =
        compute_contribution_from_attributions(&db.0, repo_id, &commit_sha).await
    {
        if let Err(e) = store_contribution_stats(&db.0, repo_id, &commit_sha, None, &stats).await {
            eprintln!("Failed to cache stats: {}", e);
        }
        return Ok(stats);
    }

    // Get linked session for this commit
    let session = match fetch_linked_session(&db.0, repo_id, &commit_sha).await {
        Ok(s) => s,
        Err(_) => {
            // No linked session - return human-only stats
            return Ok(compute_human_contribution(0));
        }
    };

    // Get commit files for overlap calculation
    let commit_files: Vec<String> = super::stats::fetch_commit_files(&db.0, repo_id, &commit_sha)
        .await
        .unwrap_or_default();

    // Compute stats
    let stats = super::session_stats::compute_session_contribution(&session, &commit_files);

    // Cache for next time
    let session_id = session.id.clone();
    if let Err(e) =
        store_contribution_stats(&db.0, repo_id, &commit_sha, Some(&session_id), &stats).await
    {
        eprintln!("Failed to cache stats: {}", e);
    }

    Ok(stats)
}

/// Get source lens for a file (Source Lens)
///
/// Returns paginated source attribution for a file at a specific commit.
/// Shows which lines were authored by agents vs humans.
#[tauri::command(rename_all = "camelCase")]
pub async fn get_file_source_lens(
    db: State<'_, DbState>,
    request: super::models::SourceLensRequest,
) -> Result<super::models::SourceLensPage, String> {
    super::source_lens::get_file_source_lens(
        &db.0,
        request.repo_id,
        &request.commit_sha,
        &request.file_path,
        request.offset,
        request.limit,
    )
    .await
}

/// Import a single attribution note from git notes into local storage
#[tauri::command(rename_all = "camelCase")]
pub async fn import_attribution_note(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_sha: String,
) -> Result<AttributionNoteImportSummary, String> {
    super::notes_io::import_attribution_note(&db.0, repo_id, commit_sha).await
}

/// Import multiple attribution notes from git notes into local storage
#[tauri::command(rename_all = "camelCase")]
pub async fn import_attribution_notes_batch(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_shas: Vec<String>,
) -> Result<AttributionNoteBatchSummary, String> {
    super::notes_io::import_attribution_notes_batch(&db.0, repo_id, commit_shas).await
}

/// Export local attribution data into git notes
#[tauri::command(rename_all = "camelCase")]
pub async fn export_attribution_note(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_sha: String,
) -> Result<AttributionNoteExportSummary, String> {
    super::notes_io::export_attribution_note(&db.0, repo_id, commit_sha).await
}

/// Compute and cache stats for a batch of commits
///
/// Useful for pre-computing stats after importing many sessions.
#[tauri::command(rename_all = "camelCase")]
pub async fn compute_stats_batch(
    db: State<'_, DbState>,
    repo_id: i64,
    commit_shas: Vec<String>,
) -> Result<usize, String> {
    use super::line_attribution::ensure_line_attributions_for_commit;
    use super::session_stats::{compute_session_contribution, store_contribution_stats};
    use super::stats::fetch_commit_files;

    let mut computed = 0;

    for commit_sha in commit_shas {
        let _ = ensure_line_attributions_for_commit(&db.0, repo_id, &commit_sha).await;

        // Check if already cached
        if fetch_cached_stats(&db.0, repo_id, &commit_sha)
            .await
            .is_some()
        {
            continue;
        }

        if let Ok(Some(stats)) =
            compute_contribution_from_attributions(&db.0, repo_id, &commit_sha).await
        {
            if store_contribution_stats(&db.0, repo_id, &commit_sha, None, &stats)
                .await
                .is_ok()
            {
                computed += 1;
            }
            continue;
        }

        // Try to get linked session
        let session = match fetch_linked_session(&db.0, repo_id, &commit_sha).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Get commit files
        let commit_files: Vec<String> = fetch_commit_files(&db.0, repo_id, &commit_sha)
            .await
            .unwrap_or_default();

        // Compute and store
        let stats = compute_session_contribution(&session, &commit_files);
        let session_id = session.id.clone();

        if store_contribution_stats(&db.0, repo_id, &commit_sha, Some(&session_id), &stats)
            .await
            .is_ok()
        {
            computed += 1;
        }
    }

    Ok(computed)
}
