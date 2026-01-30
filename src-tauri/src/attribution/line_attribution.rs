//! Line attribution storage and retrieval

use super::git_utils::{collect_changed_ranges, compute_rewrite_key, list_commit_files};
use super::stats::LinkedSessionRow;
use super::utils::fetch_repo_root;
use git2::Repository;

/// Database row for line attribution commit
#[derive(sqlx::FromRow)]
pub struct LineAttributionCommitRow {
    pub file_path: String,
    pub start_line: i32,
    pub end_line: i32,
    pub session_id: Option<String>,
    pub author_type: String,
    pub ai_percentage: Option<i32>,
    pub tool: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Copy)]
pub struct ChangedRange {
    pub start_line: i32,
    pub end_line: i32,
    pub kind: ChangeKind,
}

#[derive(Clone, Copy)]
pub enum ChangeKind {
    Added,
    Modified,
}

/// Ensure line attributions exist for a commit
pub async fn ensure_line_attributions_for_commit(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<(), String> {
    if line_attributions_exist(db, repo_id, commit_sha).await? {
        return Ok(());
    }

    if let Ok(true) = try_restore_attributions_via_rewrite_key(db, repo_id, commit_sha).await {
        return Ok(());
    }

    let sessions = fetch_sessions_for_commit(db, repo_id, commit_sha).await?;
    if sessions.is_empty() {
        return Ok(());
    }

    let repo_root = fetch_repo_root(db, repo_id).await?;
    let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;
    let commit_files = list_commit_files(&repo, commit_sha)?;
    let session_files = sessions
        .iter()
        .map(|session| parse_session_files(&session.files))
        .collect::<Vec<_>>();

    for file_path in commit_files {
        let matched_indexes = session_files
            .iter()
            .enumerate()
            .filter_map(|(idx, files)| files.contains(&file_path).then_some(idx))
            .collect::<Vec<_>>();
        let target_indexes = if matched_indexes.is_empty() {
            vec![0]
        } else {
            matched_indexes
        };

        let ranges = collect_changed_ranges(&repo, commit_sha, &file_path)?;
        for range in &ranges {
            let (author_type, ai_percentage) = match range.kind {
                ChangeKind::Added => ("ai_agent", None),
                ChangeKind::Modified => ("mixed", Some(50.0)),
            };
            for session_index in &target_indexes {
                let session = &sessions[*session_index];
                sqlx::query(
                    r#"
                    INSERT INTO line_attributions (
                        repo_id,
                        commit_sha,
                        file_path,
                        start_line,
                        end_line,
                        session_id,
                        author_type,
                        ai_percentage,
                        tool,
                        model
                    )
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(repo_id)
                .bind(commit_sha)
                .bind(&file_path)
                .bind(range.start_line)
                .bind(range.end_line)
                .bind(&session.session_id)
                .bind(author_type)
                .bind(ai_percentage)
                .bind(&session.tool)
                .bind(&session.model)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
    }

    let _ = store_rewrite_key_for_commit(db, repo_id, commit_sha).await;

    Ok(())
}

/// Check if line attributions exist for a commit
async fn line_attributions_exist(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<bool, String> {
    let exists: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT 1
        FROM line_attributions
        WHERE repo_id = ? AND commit_sha = ?
        LIMIT 1
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(exists.is_some())
}

/// Try to restore attributions from a similar commit via rewrite key
async fn try_restore_attributions_via_rewrite_key(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<bool, String> {
    use super::stats::compute_contribution_from_attributions;
    use super::session_stats::store_contribution_stats;

    let Some(rewrite_key) = store_rewrite_key_for_commit(db, repo_id, commit_sha).await? else {
        return Ok(false);
    };

    let Some(source_commit) =
        find_commit_by_rewrite_key(db, repo_id, &rewrite_key, commit_sha).await?
    else {
        return Ok(false);
    };

    let copied = copy_line_attributions(db, repo_id, &source_commit, commit_sha).await?;
    if copied == 0 {
        return Ok(false);
    }

    if let Ok(Some(stats)) = compute_contribution_from_attributions(db, repo_id, commit_sha).await {
        let _ = store_contribution_stats(db, repo_id, commit_sha, None, &stats).await;
    }

    Ok(true)
}

/// Store rewrite key for a commit
async fn store_rewrite_key_for_commit(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Option<String>, String> {
    const REWRITE_KEY_ALGORITHM: &str = "patch-id";
    let repo_root = fetch_repo_root(db, repo_id).await?;
    let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;
    let rewrite_key = compute_rewrite_key(&repo, commit_sha).ok();
    store_rewrite_key(
        db,
        repo_id,
        commit_sha,
        rewrite_key.as_deref(),
        Some(REWRITE_KEY_ALGORITHM),
    )
    .await?;
    Ok(rewrite_key)
}

/// Store a rewrite key in the database
pub async fn store_rewrite_key(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
    rewrite_key: Option<&str>,
    algorithm: Option<&str>,
) -> Result<(), String> {
    const REWRITE_KEY_ALGORITHM: &str = "patch-id";
    let Some(rewrite_key) = rewrite_key else {
        return Ok(());
    };
    let algorithm = algorithm.unwrap_or(REWRITE_KEY_ALGORITHM);

    sqlx::query(
        r#"
        INSERT INTO commit_rewrite_keys (repo_id, commit_sha, rewrite_key, algorithm)
        VALUES (?, ?, ?, ?)
        ON CONFLICT(repo_id, commit_sha) DO UPDATE SET
            rewrite_key = excluded.rewrite_key,
            algorithm = excluded.algorithm,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .bind(rewrite_key)
    .bind(algorithm)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Find a commit by its rewrite key (excluding a specific commit)
async fn find_commit_by_rewrite_key(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    rewrite_key: &str,
    exclude_commit: &str,
) -> Result<Option<String>, String> {
    sqlx::query_scalar(
        r#"
        SELECT rk.commit_sha
        FROM commit_rewrite_keys rk
        WHERE rk.repo_id = ?
          AND rk.rewrite_key = ?
          AND rk.commit_sha != ?
          AND EXISTS (
            SELECT 1
            FROM line_attributions la
            WHERE la.repo_id = rk.repo_id AND la.commit_sha = rk.commit_sha
          )
        ORDER BY rk.updated_at DESC, rk.created_at DESC
        LIMIT 1
        "#,
    )
    .bind(repo_id)
    .bind(rewrite_key)
    .bind(exclude_commit)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())
}

/// Copy line attributions from one commit to another
async fn copy_line_attributions(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    source_commit: &str,
    target_commit: &str,
) -> Result<u32, String> {
    let result = sqlx::query(
        r#"
        INSERT INTO line_attributions (
            repo_id,
            commit_sha,
            file_path,
            start_line,
            end_line,
            session_id,
            author_type,
            ai_percentage,
            tool,
            model
        )
        SELECT
            repo_id,
            ?,
            file_path,
            start_line,
            end_line,
            session_id,
            author_type,
            ai_percentage,
            tool,
            model
        FROM line_attributions
        WHERE repo_id = ? AND commit_sha = ?
        "#,
    )
    .bind(target_commit)
    .bind(repo_id)
    .bind(source_commit)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(result.rows_affected() as u32)
}

/// Fetch sessions linked to a commit
async fn fetch_sessions_for_commit(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Vec<LinkedSessionRow>, String> {
    sqlx::query_as::<_, LinkedSessionRow>(
        r#"
        SELECT s.id as session_id, s.tool, s.model, s.files
        FROM session_links l
        JOIN sessions s ON s.id = l.session_id
        WHERE l.repo_id = ? AND l.commit_sha = ?
        ORDER BY l.confidence DESC, l.created_at DESC
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())
}

/// Parse session files JSON
pub fn parse_session_files(raw: &Option<String>) -> std::collections::HashSet<String> {
    raw.as_ref()
        .and_then(|value| serde_json::from_str::<Vec<String>>(value).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

/// Fetch line attributions for a specific file
pub async fn fetch_line_attributions(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
    file_path: &str,
) -> Result<Vec<super::source_lens::LineAttributionRow>, String> {
    sqlx::query_as::<_, super::source_lens::LineAttributionRow>(
        r#"
        SELECT
            la.start_line,
            la.end_line,
            la.session_id,
            la.author_type,
            la.ai_percentage,
            la.tool,
            la.model,
            COALESCE(s.trace_available, 0) as trace_available
        FROM line_attributions la
        LEFT JOIN sessions s ON s.id = la.session_id
        WHERE la.repo_id = ? AND la.commit_sha = ? AND la.file_path = ?
        ORDER BY la.start_line
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .bind(file_path)
    .fetch_all(db)
    .await
    .map_err(|e| format!("Database error: {}", e))
}

/// Fetch all line attributions for a commit
pub async fn fetch_line_attributions_for_commit(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Vec<LineAttributionCommitRow>, String> {
    sqlx::query_as::<_, LineAttributionCommitRow>(
        r#"
        SELECT
            file_path,
            start_line,
            end_line,
            session_id,
            author_type,
            ai_percentage,
            tool,
            model
        FROM line_attributions
        WHERE repo_id = ? AND commit_sha = ?
        ORDER BY file_path, start_line
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .fetch_all(db)
    .await
    .map_err(|e| format!("Database error: {}", e))
}
