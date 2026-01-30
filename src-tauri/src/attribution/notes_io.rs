//! Git note import/export functionality

use super::line_attribution::store_rewrite_key;
use super::notes::{
    build_attribution_note, parse_attribution_note, NoteFile, NoteRange, NoteSourceMeta,
    ParsedAttributionNote, ATTRIBUTION_NOTES_REF, LEGACY_ATTRIBUTION_NOTES_REF,
};
use super::stats::compute_contribution_from_attributions;
use super::utils::{fetch_repo_root, fetch_session_meta};
use git2::{Oid, Repository, Signature};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionNoteImportSummary {
    pub commit_sha: String,
    pub status: String,
    pub imported_ranges: u32,
    pub imported_sessions: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionNoteBatchSummary {
    pub total: u32,
    pub imported: u32,
    pub missing: u32,
    pub failed: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionNoteExportSummary {
    pub commit_sha: String,
    pub status: String,
}

const REWRITE_KEY_ALGORITHM: &str = "patch-id";

/// Import a single attribution note from git notes into local storage
pub async fn import_attribution_note(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: String,
) -> Result<AttributionNoteImportSummary, String> {
    import_attribution_note_internal(db, repo_id, &commit_sha).await
}

/// Import multiple attribution notes from git notes into local storage
pub async fn import_attribution_notes_batch(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_shas: Vec<String>,
) -> Result<AttributionNoteBatchSummary, String> {
    let mut imported = 0;
    let mut missing = 0;
    let mut failed = 0;

    for commit_sha in commit_shas {
        match import_attribution_note_internal(db, repo_id, &commit_sha).await {
            Ok(summary) => {
                if summary.status == "imported" {
                    imported += 1;
                } else {
                    missing += 1;
                }
            }
            Err(_) => {
                failed += 1;
            }
        }
    }

    Ok(AttributionNoteBatchSummary {
        total: (imported + missing + failed) as u32,
        imported: imported as u32,
        missing: missing as u32,
        failed: failed as u32,
    })
}

/// Export local attribution data into git notes
pub async fn export_attribution_note(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: String,
) -> Result<AttributionNoteExportSummary, String> {
    let summary = export_attribution_note_internal(db, repo_id, &commit_sha).await?;
    Ok(summary)
}

/// Internal implementation for importing a single attribution note
pub async fn import_attribution_note_internal(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<AttributionNoteImportSummary, String> {
    use super::session_stats::store_contribution_stats;

    let repo_root = fetch_repo_root(db, repo_id).await?;

    // Parse the note in a separate block to ensure repo/note are dropped before await
    let parsed = {
        let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;
        let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;

        let note = match repo
            .find_note(Some(ATTRIBUTION_NOTES_REF), oid)
            .or_else(|_| repo.find_note(Some(LEGACY_ATTRIBUTION_NOTES_REF), oid))
        {
            Ok(note) => note,
            Err(_) => {
                return Ok(AttributionNoteImportSummary {
                    commit_sha: commit_sha.to_string(),
                    status: "missing".to_string(),
                    imported_ranges: 0,
                    imported_sessions: 0,
                });
            }
        };

        let message = note
            .message()
            .ok_or_else(|| "Attribution note is not valid UTF-8".to_string())?
            .to_string();

        // note and repo are dropped here, before the await below
        parse_attribution_note(&message)
    };

    if parsed.files.is_empty() {
        return Ok(AttributionNoteImportSummary {
            commit_sha: commit_sha.to_string(),
            status: "missing".to_string(),
            imported_ranges: 0,
            imported_sessions: 0,
        });
    }

    let (ranges, sessions) =
        store_line_attributions_from_note(db, repo_id, commit_sha, &parsed).await?;

    let _ = store_rewrite_key_from_note(db, repo_id, commit_sha, &parsed).await;

    if let Ok(Some(stats)) = compute_contribution_from_attributions(db, repo_id, commit_sha).await {
        let _ = store_contribution_stats(db, repo_id, commit_sha, None, &stats).await;
    }

    Ok(AttributionNoteImportSummary {
        commit_sha: commit_sha.to_string(),
        status: "imported".to_string(),
        imported_ranges: ranges,
        imported_sessions: sessions,
    })
}

/// Store rewrite key from parsed attribution note
async fn store_rewrite_key_from_note(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
    parsed: &ParsedAttributionNote,
) -> Result<(), String> {
    use super::git_utils::compute_rewrite_key;

    let algorithm = parsed
        .rewrite_algorithm
        .as_deref()
        .unwrap_or(REWRITE_KEY_ALGORITHM);

    if parsed.rewrite_key.is_some() {
        return store_rewrite_key(
            db,
            repo_id,
            commit_sha,
            parsed.rewrite_key.as_deref(),
            Some(algorithm),
        )
        .await;
    }

    let repo_root = fetch_repo_root(db, repo_id).await?;
    let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;
    let rewrite_key = compute_rewrite_key(&repo, commit_sha).ok();
    store_rewrite_key(
        db,
        repo_id,
        commit_sha,
        rewrite_key.as_deref(),
        Some(algorithm),
    )
    .await
}

/// Store line attributions from parsed attribution note
async fn store_line_attributions_from_note(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
    parsed: &ParsedAttributionNote,
) -> Result<(u32, u32), String> {
    sqlx::query(
        r#"
        DELETE FROM line_attributions
        WHERE repo_id = ? AND commit_sha = ?
        "#,
    )
    .bind(repo_id)
    .bind(commit_sha)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    let mut range_count = 0;
    let mut session_ids: std::collections::HashMap<String, ()> = std::collections::HashMap::new();

    for file in &parsed.files {
        for range in &file.ranges {
            let mut meta = parsed
                .sources
                .get(&range.session_id)
                .cloned()
                .unwrap_or_default();

            if meta.tool.is_none() || meta.model.is_none() || meta.conversation_id.is_none() {
                if let Ok(Some(session)) = fetch_session_meta(db, &range.session_id).await {
                    if meta.tool.is_none() {
                        meta.tool = session.tool;
                    }
                    if meta.model.is_none() {
                        meta.model = session.model;
                    }
                    if meta.conversation_id.is_none() {
                        meta.conversation_id = session.conversation_id;
                    }
                }
            }

            let author_type = match meta.checkpoint_kind.as_deref() {
                Some("ai_tab") | Some("ai_assist") => "ai_tab",
                _ => "ai_agent",
            };

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
            .bind(&file.path)
            .bind(range.start_line)
            .bind(range.end_line)
            .bind(&range.session_id)
            .bind(author_type)
            .bind(None::<f32>)
            .bind(&meta.tool)
            .bind(&meta.model)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;

            range_count += 1;
            session_ids.insert(range.session_id.clone(), ());
        }
    }

    Ok((range_count, session_ids.len() as u32))
}

/// Internal implementation for exporting attribution to git note
async fn export_attribution_note_internal(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<AttributionNoteExportSummary, String> {
    use super::git_utils::compute_rewrite_key;
    use super::line_attribution::fetch_line_attributions_for_commit;

    let rows = fetch_line_attributions_for_commit(db, repo_id, commit_sha).await?;
    if rows.is_empty() {
        return Ok(AttributionNoteExportSummary {
            commit_sha: commit_sha.to_string(),
            status: "empty".to_string(),
        });
    }

    let mut files_map: std::collections::HashMap<String, NoteFile> = std::collections::HashMap::new();
    let mut sources: std::collections::HashMap<String, NoteSourceMeta> = std::collections::HashMap::new();

    for row in rows {
        let Some(session_id) = row.session_id.clone() else {
            continue;
        };

        let file_entry = files_map.entry(row.file_path.clone()).or_insert(NoteFile {
            path: row.file_path.clone(),
            ranges: Vec::new(),
        });

        file_entry.ranges.push(NoteRange {
            session_id: session_id.clone(),
            start_line: row.start_line,
            end_line: row.end_line,
        });

        let source = sources.entry(session_id.clone()).or_default();
        if source.tool.is_none() {
            source.tool = row.tool.clone();
        }
        if source.model.is_none() {
            source.model = row.model.clone();
        }
        if source.checkpoint_kind.is_none() {
            source.checkpoint_kind = Some(match row.author_type.as_str() {
                "ai_tab" => "ai_tab".to_string(),
                _ => "ai_agent".to_string(),
            });
        }
    }

    for (session_id, source) in sources.iter_mut() {
        if let Ok(Some(meta)) = fetch_session_meta(db, session_id).await {
            if source.tool.is_none() {
                source.tool = meta.tool;
            }
            if source.model.is_none() {
                source.model = meta.model;
            }
            if source.conversation_id.is_none() {
                source.conversation_id = meta.conversation_id;
            }
        }
    }

    let files = files_map.into_values().collect::<Vec<_>>();

    let repo_root = fetch_repo_root(db, repo_id).await?;
    let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;
    let rewrite_key = compute_rewrite_key(&repo, commit_sha).ok();
    let _ = store_rewrite_key(
        db,
        repo_id,
        commit_sha,
        rewrite_key.as_deref(),
        Some(REWRITE_KEY_ALGORITHM),
    )
    .await;

    let note_text = build_attribution_note(
        commit_sha,
        &files,
        &sources,
        rewrite_key.as_deref(),
        Some(REWRITE_KEY_ALGORITHM),
    );
    let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;

    let signature = repo
        .signature()
        .or_else(|_| Signature::now("Narrative", "narrative@local"))
        .map_err(|e| e.to_string())?;

    repo.note(
        &signature,
        &signature,
        Some(ATTRIBUTION_NOTES_REF),
        oid,
        &note_text,
        true,
    )
    .map_err(|e| e.to_string())?;

    Ok(AttributionNoteExportSummary {
        commit_sha: commit_sha.to_string(),
        status: "exported".to_string(),
    })
}
