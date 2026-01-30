//! Shared utilities

/// Fetch repository root path from database
pub async fn fetch_repo_root(db: &sqlx::SqlitePool, repo_id: i64) -> Result<String, String> {
    let path: String = sqlx::query_scalar(
        r#"
        SELECT path FROM repos WHERE id = ?
        "#,
    )
    .bind(repo_id)
    .fetch_one(db)
    .await
    .map_err(|e| format!("Failed to load repo path: {}", e))?;

    Ok(path)
}

#[derive(sqlx::FromRow)]
pub struct SessionMetaRow {
    pub tool: Option<String>,
    pub model: Option<String>,
    pub conversation_id: Option<String>,
    #[allow(dead_code)]
    pub trace_available: Option<i32>,
}

/// Fetch session metadata
pub async fn fetch_session_meta(
    db: &sqlx::SqlitePool,
    session_id: &str,
) -> Result<Option<SessionMetaRow>, String> {
    sqlx::query_as::<_, SessionMetaRow>(
        r#"
        SELECT tool, model, conversation_id, trace_available
        FROM sessions
        WHERE id = ?
        "#,
    )
    .bind(session_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())
}
