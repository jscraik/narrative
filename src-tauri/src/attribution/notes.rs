use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Notes ref for Narrative attribution exports/imports.
pub const ATTRIBUTION_NOTES_REF: &str = "refs/notes/narrative-attribution";
/// Legacy notes ref for backward-compatible imports (read-only).
pub const LEGACY_ATTRIBUTION_NOTES_REF: &str = "refs/notes/ai";
pub const ATTRIBUTION_SCHEMA_VERSION: &str = "narrative/attribution/1.0.0";

#[derive(Debug, Clone)]
pub struct NoteRange {
    pub session_id: String,
    pub start_line: i32,
    pub end_line: i32,
}

#[derive(Debug, Clone)]
pub struct NoteFile {
    pub path: String,
    pub ranges: Vec<NoteRange>,
}

#[derive(Debug, Clone, Default)]
pub struct NoteSourceMeta {
    pub tool: Option<String>,
    pub model: Option<String>,
    pub checkpoint_kind: Option<String>,
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedAttributionNote {
    pub files: Vec<NoteFile>,
    pub sources: HashMap<String, NoteSourceMeta>,
    pub rewrite_key: Option<String>,
    pub rewrite_algorithm: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NotePayload {
    #[serde(rename = "schema_version")]
    #[allow(dead_code)]
    schema_version: Option<String>,
    #[serde(rename = "base_commit_sha")]
    #[allow(dead_code)]
    base_commit_sha: Option<String>,
    #[serde(rename = "rewrite_key")]
    rewrite_key: Option<String>,
    #[serde(rename = "rewrite_algorithm")]
    rewrite_algorithm: Option<String>,
    #[serde(alias = "prompts")]
    sources: Option<HashMap<String, NoteSourcePayload>>,
}

#[derive(Debug, Deserialize)]
struct NoteSourcePayload {
    #[serde(rename = "agent_id")]
    agent_id: Option<NoteAgentId>,
    checkpoint_kind: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NoteAgentId {
    tool: Option<String>,
    id: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Serialize)]
struct AttributionNotePayload {
    #[serde(rename = "schema_version")]
    schema_version: String,
    #[serde(rename = "base_commit_sha")]
    base_commit_sha: String,
    #[serde(rename = "rewrite_key")]
    #[serde(skip_serializing_if = "Option::is_none")]
    rewrite_key: Option<String>,
    #[serde(rename = "rewrite_algorithm")]
    #[serde(skip_serializing_if = "Option::is_none")]
    rewrite_algorithm: Option<String>,
    #[serde(rename = "prompts")]
    sources: HashMap<String, AttributionNoteSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    messages_redacted: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AttributionNoteSource {
    #[serde(rename = "agent_id")]
    agent_id: Option<AttributionAgentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    checkpoint_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    messages_redacted: Option<bool>,
}

#[derive(Debug, Serialize)]
struct AttributionAgentId {
    tool: Option<String>,
    id: Option<String>,
    model: Option<String>,
}

pub fn parse_attribution_note(message: &str) -> ParsedAttributionNote {
    let mut files: Vec<NoteFile> = Vec::new();
    let mut current_file: Option<NoteFile> = None;
    let mut json_lines: Vec<String> = Vec::new();
    let mut in_json = false;

    for line in message.lines() {
        if !in_json && line.trim() == "---" {
            in_json = true;
            continue;
        }

        if in_json {
            json_lines.push(line.to_string());
            continue;
        }

        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            continue;
        }

        if !trimmed.starts_with(' ') && !trimmed.starts_with('\t') {
            if let Some(file) = current_file.take() {
                files.push(file);
            }
            current_file = Some(NoteFile {
                path: trimmed.trim().to_string(),
                ranges: Vec::new(),
            });
            continue;
        }

        let Some(file) = current_file.as_mut() else {
            continue;
        };

        let mut parts = trimmed.split_whitespace();
        let Some(session_id) = parts.next() else {
            continue;
        };
        let range_text = parts.collect::<Vec<_>>().join(" ");
        if range_text.is_empty() {
            continue;
        }

        for segment in range_text.split(',') {
            let seg = segment.trim();
            if seg.is_empty() {
                continue;
            }
            if let Some((start, end)) = seg.split_once('-') {
                let start_line = start.trim().parse::<i32>().unwrap_or(0);
                let end_line = end.trim().parse::<i32>().unwrap_or(start_line);
                if start_line > 0 {
                    file.ranges.push(NoteRange {
                        session_id: session_id.to_string(),
                        start_line,
                        end_line,
                    });
                }
            } else {
                let line_num = seg.parse::<i32>().unwrap_or(0);
                if line_num > 0 {
                    file.ranges.push(NoteRange {
                        session_id: session_id.to_string(),
                        start_line: line_num,
                        end_line: line_num,
                    });
                }
            }
        }
    }

    if let Some(file) = current_file.take() {
        files.push(file);
    }

    let json_text = json_lines.join("\n").trim().to_string();
    let mut sources: HashMap<String, NoteSourceMeta> = HashMap::new();

    let mut rewrite_key: Option<String> = None;
    let mut rewrite_algorithm: Option<String> = None;

    if !json_text.is_empty() {
        if let Ok(payload) = serde_json::from_str::<NotePayload>(&json_text) {
            rewrite_key = payload.rewrite_key.clone();
            rewrite_algorithm = payload.rewrite_algorithm.clone();
            if let Some(map) = payload.sources {
                for (session_id, source) in map {
                    let tool = source.agent_id.as_ref().and_then(|id| id.tool.clone());
                    let model = source
                        .agent_id
                        .as_ref()
                        .and_then(|id| id.model.clone())
                        .or(source.model.clone());
                    let conversation_id = source.agent_id.and_then(|id| id.id);
                    sources.insert(
                        session_id,
                        NoteSourceMeta {
                            tool,
                            model,
                            checkpoint_kind: source.checkpoint_kind,
                            conversation_id,
                        },
                    );
                }
            }
        }
    }

    ParsedAttributionNote {
        files,
        sources,
        rewrite_key,
        rewrite_algorithm,
    }
}

pub fn build_attribution_note(
    commit_sha: &str,
    files: &[NoteFile],
    sources: &HashMap<String, NoteSourceMeta>,
    rewrite_key: Option<&str>,
    rewrite_algorithm: Option<&str>,
) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut sorted_files = files.to_vec();
    sorted_files.sort_by(|a, b| a.path.cmp(&b.path));

    for file in &sorted_files {
        lines.push(file.path.clone());

        let mut ranges_by_session: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
        for range in &file.ranges {
            ranges_by_session
                .entry(range.session_id.clone())
                .or_default()
                .push((range.start_line, range.end_line));
        }

        let mut session_ids: Vec<String> = ranges_by_session.keys().cloned().collect();
        session_ids.sort();

        for session_id in session_ids {
            let ranges = ranges_by_session.remove(&session_id).unwrap_or_default();
            let merged = merge_ranges(ranges);
            let range_text = merged
                .iter()
                .map(|(start, end)| {
                    if start == end {
                        format!("{start}")
                    } else {
                        format!("{start}-{end}")
                    }
                })
                .collect::<Vec<_>>()
                .join(",");
            lines.push(format!("  {session_id} {range_text}"));
        }
    }

    let payload = AttributionNotePayload {
        schema_version: ATTRIBUTION_SCHEMA_VERSION.to_string(),
        base_commit_sha: commit_sha.to_string(),
        rewrite_key: rewrite_key.map(|value| value.to_string()),
        rewrite_algorithm: rewrite_algorithm.map(|value| value.to_string()),
        sources: build_sources_payload(sources),
        messages_redacted: Some(true),
    };

    let json = serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string());
    lines.push("---".to_string());
    lines.push(json);
    lines.join("\n")
}

fn merge_ranges(mut ranges: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: Vec<(i32, i32)> = Vec::new();

    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 + 1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }

    merged
}

fn build_sources_payload(
    sources: &HashMap<String, NoteSourceMeta>,
) -> HashMap<String, AttributionNoteSource> {
    let mut out = HashMap::new();
    for (session_id, meta) in sources {
        let agent_id = Some(AttributionAgentId {
            tool: meta.tool.clone(),
            id: meta
                .conversation_id
                .clone()
                .or_else(|| Some(session_id.clone())),
            model: meta.model.clone(),
        });
        out.insert(
            session_id.clone(),
            AttributionNoteSource {
                agent_id,
                checkpoint_kind: meta.checkpoint_kind.clone(),
                messages_redacted: Some(true),
            },
        );
    }
    out
}
