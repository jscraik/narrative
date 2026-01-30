//! Source lens (line attribution display) with pagination

use super::line_attribution::ensure_line_attributions_for_commit;
use super::models::{SourceLensPage, SourceLine};
use super::{line_attribution::fetch_line_attributions, utils::fetch_repo_root};
use git2::Repository;

/// Database row for line attribution
#[derive(sqlx::FromRow)]
pub struct LineAttributionRow {
    pub start_line: i32,
    pub end_line: i32,
    pub session_id: Option<String>,
    pub author_type: String,
    pub ai_percentage: Option<i32>,
    pub tool: Option<String>,
    pub model: Option<String>,
    pub trace_available: i32,
}

/// Get source lens for a file (Source Lens)
///
/// Returns paginated source attribution for a file at a specific commit.
/// Shows which lines were authored by agents vs humans.
pub async fn get_file_source_lens(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
    file_path: &str,
    offset: u32,
    limit: u32,
) -> Result<SourceLensPage, String> {
    let _ = ensure_line_attributions_for_commit(db, repo_id, commit_sha).await;

    let repo_root = fetch_repo_root(db, repo_id).await?;
    let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;
    let file_lines = load_file_lines(&repo, commit_sha, file_path)?;

    if file_lines.is_empty() {
        return Ok(SourceLensPage {
            lines: Vec::new(),
            total_lines: 0,
            has_more: false,
        });
    }

    let attributions = fetch_line_attributions(db, repo_id, commit_sha, file_path).await?;
    let line_meta = build_line_meta(file_lines.len(), &attributions);

    let total_lines = file_lines.len() as u32;
    let start = offset as usize;
    if start >= file_lines.len() {
        return Ok(SourceLensPage {
            lines: Vec::new(),
            total_lines,
            has_more: false,
        });
    }

    let end = (offset + limit) as usize;
    let slice = &file_lines[start..end.min(file_lines.len())];
    let lines: Vec<SourceLine> = slice
        .iter()
        .enumerate()
        .map(|(idx, content)| {
            let line_index = start + idx;
            let meta = line_meta.get(line_index).cloned().unwrap_or_default();
            SourceLine {
                line_number: (line_index + 1) as u32,
                content: content.clone(),
                author_type: meta.author_type,
                session_id: meta.session_id,
                ai_percentage: meta.ai_percentage,
                tool: meta.tool,
                model: meta.model,
                trace_available: meta.trace_available,
            }
        })
        .collect();

    let has_more = end < file_lines.len();

    Ok(SourceLensPage {
        lines,
        total_lines,
        has_more,
    })
}

/// Load file content from git repository at specific commit
pub fn load_file_lines(
    repo: &Repository,
    commit_sha: &str,
    file_path: &str,
) -> Result<Vec<String>, String> {
    use git2::Oid;
    use std::path::Path;

    let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;
    let entry = tree
        .get_path(Path::new(file_path))
        .map_err(|e| e.to_string())?;
    let object = entry.to_object(repo).map_err(|e| e.to_string())?;
    let blob = object
        .as_blob()
        .ok_or_else(|| "File is not a blob".to_string())?;
    let content = String::from_utf8_lossy(blob.content());
    Ok(content.lines().map(|line| line.to_string()).collect())
}

#[derive(Clone)]
pub struct LineMeta {
    pub author_type: String,
    pub session_id: Option<String>,
    pub ai_percentage: Option<u8>,
    pub tool: Option<String>,
    pub model: Option<String>,
    pub trace_available: bool,
}

impl Default for LineMeta {
    fn default() -> Self {
        Self {
            author_type: "human".to_string(),
            session_id: None,
            ai_percentage: None,
            tool: None,
            model: None,
            trace_available: false,
        }
    }
}

/// Build line metadata from attribution rows
pub fn build_line_meta(total_lines: usize, attrs: &[LineAttributionRow]) -> Vec<LineMeta> {
    let mut lines = vec![LineMeta::default(); total_lines];

    for attr in attrs {
        let start = attr.start_line.max(1) as usize;
        let end = attr.end_line.max(start as i32) as usize;
        for line_num in start..=end {
            if line_num == 0 || line_num > total_lines {
                continue;
            }
            let idx = line_num - 1;
            let meta = &mut lines[idx];
            apply_line_attr(meta, attr);
        }
    }

    lines
}

/// Apply a single line attribution to line metadata
fn apply_line_attr(meta: &mut LineMeta, attr: &LineAttributionRow) {
    let incoming = attr.author_type.as_str();
    let incoming_kind = if incoming.is_empty() {
        "human"
    } else {
        incoming
    };
    let incoming_trace = attr.trace_available > 0;

    if meta.author_type == "human" {
        meta.author_type = incoming_kind.to_string();
        meta.session_id = attr.session_id.clone();
        meta.ai_percentage = attr.ai_percentage.map(|v| v as u8);
        meta.tool = attr.tool.clone();
        meta.model = attr.model.clone();
        meta.trace_available = incoming_trace;
        return;
    }

    let should_mix = meta.author_type != incoming_kind
        || (incoming_kind != "mixed"
            && meta.session_id.is_some()
            && attr.session_id.is_some()
            && meta.session_id != attr.session_id);

    if should_mix {
        meta.author_type = "mixed".to_string();
        meta.ai_percentage = Some(50);
    } else if incoming_kind == "mixed" {
        if let Some(value) = attr.ai_percentage {
            meta.ai_percentage = Some(value as u8);
        }
    }

    if meta.session_id.is_none() {
        meta.session_id = attr.session_id.clone();
    }
    if meta.tool.is_none() {
        meta.tool = attr.tool.clone();
    }
    if meta.model.is_none() {
        meta.model = attr.model.clone();
    }
    meta.trace_available = meta.trace_available || incoming_trace;
}
