//! Attribution note metadata storage

use sqlx::SqlitePool;

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
pub struct AttributionNoteMetaRow {
    pub commit_sha: String,
    pub note_ref: String,
    pub note_hash: String,
    pub schema_version: Option<String>,
    pub metadata_available: i32,
    pub metadata_cached: i32,
    pub prompt_count: Option<i32>,
    pub updated_at: String,
}

pub struct AttributionNoteMetaInput {
    pub note_ref: String,
    pub note_hash: String,
    pub schema_version: Option<String>,
    pub metadata_available: bool,
    pub metadata_cached: bool,
    pub prompt_count: usize,
}

pub async fn upsert_attribution_note_meta(
    db: &SqlitePool,
    repo_id: i64,
    commit_sha: &str,
    input: AttributionNoteMetaInput,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO attribution_note_meta (
            repo_id,
            commit_sha,
            note_ref,
            note_hash,
            schema_version,
            metadata_available,
            metadata_cached,
            prompt_count
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(repo_id, commit_sha, note_ref) DO UPDATE SET
            note_hash = excluded.note_hash,
            schema_version = excluded.schema_version,
            metadata_available = excluded.metadata_available,
            metadata_cached = excluded.metadata_cached,
            prompt_count = excluded.prompt_count,
            updated_at = CURRENT_TIMESTAMP
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .bind(input.note_ref)
    .bind(input.note_hash)
    .bind(input.schema_version)
    .bind(if input.metadata_available { 1 } else { 0 })
    .bind(if input.metadata_cached { 1 } else { 0 })
    .bind(input.prompt_count as i32)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn fetch_attribution_note_meta(
    db: &SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Option<AttributionNoteMetaRow>, String> {
    let row = sqlx::query_as::<_, AttributionNoteMetaRow>(
        r#"
        SELECT
            commit_sha,
            note_ref,
            note_hash,
            schema_version,
            metadata_available,
            metadata_cached,
            prompt_count,
            updated_at
        FROM attribution_note_meta
        WHERE repo_id = ? AND commit_sha = ?
        LIMIT 1
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(row)
}

pub async fn clear_attribution_note_meta(
    db: &SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"
        DELETE FROM attribution_note_meta
        WHERE repo_id = ? AND commit_sha = ?
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub async fn mark_prompt_metadata_cached(
    db: &SqlitePool,
    repo_id: i64,
    commit_sha: &str,
    cached: bool,
) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE attribution_note_meta
        SET metadata_cached = ?, updated_at = CURRENT_TIMESTAMP
        WHERE repo_id = ? AND commit_sha = ?
        "#,
    )
    .bind(if cached { 1 } else { 0 })
    .bind(repo_id)
    .bind(commit_sha)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}
