//! Attribution note preferences storage

use sqlx::SqlitePool;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionPrefs {
    pub repo_id: i64,
    pub cache_prompt_metadata: bool,
    pub store_prompt_text: bool,
    pub show_line_overlays: bool,
    pub retention_days: Option<i32>,
    pub last_purged_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AttributionPrefsUpdate {
    pub cache_prompt_metadata: Option<bool>,
    pub store_prompt_text: Option<bool>,
    pub show_line_overlays: Option<bool>,
    pub retention_days: Option<i32>,
    pub clear_retention_days: Option<bool>,
}

#[derive(sqlx::FromRow)]
struct AttributionPrefsRow {
    repo_id: i64,
    cache_prompt_metadata: i32,
    store_prompt_text: i32,
    show_line_overlays: i32,
    retention_days: Option<i32>,
    last_purged_at: Option<String>,
}

impl AttributionPrefsRow {
    fn into_prefs(self) -> AttributionPrefs {
        AttributionPrefs {
            repo_id: self.repo_id,
            cache_prompt_metadata: self.cache_prompt_metadata != 0,
            store_prompt_text: self.store_prompt_text != 0,
            show_line_overlays: self.show_line_overlays != 0,
            retention_days: self.retention_days,
            last_purged_at: self.last_purged_at,
        }
    }
}

pub async fn fetch_or_create_prefs(
    db: &SqlitePool,
    repo_id: i64,
) -> Result<AttributionPrefs, String> {
    if let Some(row) = sqlx::query_as::<_, AttributionPrefsRow>(
        r#"
        SELECT repo_id, cache_prompt_metadata, store_prompt_text, show_line_overlays,
               retention_days, last_purged_at
        FROM attribution_prefs
        WHERE repo_id = ?
        "#,
    )
    .bind(repo_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?
    {
        return Ok(row.into_prefs());
    }

    sqlx::query(
        r#"
        INSERT INTO attribution_prefs (
            repo_id,
            cache_prompt_metadata,
            store_prompt_text,
            show_line_overlays
        )
        VALUES (?, 0, 0, 1)
        "#,
    )
    .bind(repo_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(AttributionPrefs {
        repo_id,
        cache_prompt_metadata: false,
        store_prompt_text: false,
        show_line_overlays: true,
        retention_days: None,
        last_purged_at: None,
    })
}

pub async fn update_prefs(
    db: &SqlitePool,
    repo_id: i64,
    update: AttributionPrefsUpdate,
) -> Result<AttributionPrefs, String> {
    let current = fetch_or_create_prefs(db, repo_id).await?;
    let retention_days = if update.clear_retention_days.unwrap_or(false) {
        None
    } else {
        update.retention_days.or(current.retention_days)
    };

    let next = AttributionPrefs {
        repo_id,
        cache_prompt_metadata: update
            .cache_prompt_metadata
            .unwrap_or(current.cache_prompt_metadata),
        store_prompt_text: update
            .store_prompt_text
            .unwrap_or(current.store_prompt_text),
        show_line_overlays: update
            .show_line_overlays
            .unwrap_or(current.show_line_overlays),
        retention_days,
        last_purged_at: current.last_purged_at.clone(),
    };

    sqlx::query(
        r#"
        UPDATE attribution_prefs
        SET cache_prompt_metadata = ?,
            store_prompt_text = ?,
            show_line_overlays = ?,
            retention_days = ?,
            updated_at = CURRENT_TIMESTAMP
        WHERE repo_id = ?
        "#,
    )
    .bind(if next.cache_prompt_metadata { 1 } else { 0 })
    .bind(if next.store_prompt_text { 1 } else { 0 })
    .bind(if next.show_line_overlays { 1 } else { 0 })
    .bind(next.retention_days)
    .bind(repo_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(next)
}
