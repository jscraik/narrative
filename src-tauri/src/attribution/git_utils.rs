//! Git utilities for diff computation and rewrite key generation

use super::line_attribution::{ChangeKind, ChangedRange};
use git2::{DiffFormat, DiffOptions, Oid, Repository};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// List files changed in a commit
pub fn list_commit_files(repo: &Repository, commit_sha: &str) -> Result<Vec<String>, String> {
    let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let mut options = DiffOptions::new();
    options.context_lines(0);

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut options))
        .map_err(|e| e.to_string())?;

    let mut paths = HashSet::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            paths.insert(path.to_string_lossy().to_string());
        }
    }

    Ok(paths.into_iter().collect())
}

#[derive(Default)]
struct RangeState {
    current_start: Option<i32>,
    previous_line: Option<i32>,
    current_kind: Option<ChangeKind>,
    saw_deletion: bool,
}

fn flush_range(state: &mut RangeState, ranges: &mut Vec<ChangedRange>) {
    if let (Some(start), Some(prev), Some(kind)) =
        (state.current_start, state.previous_line, state.current_kind)
    {
        ranges.push(ChangedRange {
            start_line: start,
            end_line: prev,
            kind,
        });
    }
    state.current_start = None;
    state.previous_line = None;
    state.current_kind = None;
}

/// Collect changed line ranges for all files in a commit
pub fn collect_changed_ranges_by_file(
    repo: &Repository,
    commit_sha: &str,
) -> Result<HashMap<String, Vec<ChangedRange>>, String> {
    let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let mut opts = DiffOptions::new();
    opts.context_lines(0);

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
        .map_err(|e| e.to_string())?;

    let mut ranges_by_file: HashMap<String, Vec<ChangedRange>> = HashMap::new();
    let mut states: HashMap<String, RangeState> = HashMap::new();

    diff.foreach(
        &mut |_delta, _progress| true,
        None,
        None,
        Some(&mut |delta, _hunk, line| {
            let path = delta.new_file().path().or_else(|| delta.old_file().path());
            let Some(path) = path else {
                return true;
            };
            let path = path.to_string_lossy().to_string();
            let ranges = ranges_by_file.entry(path.clone()).or_default();
            let state = states.entry(path).or_default();

            match line.origin() {
                '-' => {
                    flush_range(state, ranges);
                    state.saw_deletion = true;
                }
                '+' => {
                    if let Some(new_lineno) = line.new_lineno() {
                        let new_line = new_lineno as i32;
                        match (state.current_start, state.previous_line) {
                            (Some(start), Some(prev)) if new_line == prev + 1 => {
                                state.previous_line = Some(new_line);
                                state.current_start = Some(start);
                            }
                            _ => {
                                flush_range(state, ranges);
                                state.current_start = Some(new_line);
                                state.previous_line = Some(new_line);
                                state.current_kind = Some(if state.saw_deletion {
                                    ChangeKind::Modified
                                } else {
                                    ChangeKind::Added
                                });
                            }
                        }
                    }
                }
                _ => {
                    if state.current_start.is_some() {
                        flush_range(state, ranges);
                    }
                    state.saw_deletion = false;
                }
            }

            true
        }),
    )
    .map_err(|e| e.to_string())?;

    for (path, mut state) in states {
        if let Some(ranges) = ranges_by_file.get_mut(&path) {
            flush_range(&mut state, ranges);
        }
    }

    Ok(ranges_by_file)
}

/// Collect changed line ranges for a file in a commit
pub fn collect_changed_ranges(
    repo: &Repository,
    commit_sha: &str,
    file_path: &str,
) -> Result<Vec<ChangedRange>, String> {
    let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let mut opts = DiffOptions::new();
    opts.pathspec(file_path);
    opts.context_lines(0);

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
        .map_err(|e| e.to_string())?;

    let mut ranges: Vec<ChangedRange> = Vec::new();
    let mut current_start: Option<i32> = None;
    let mut previous_line: Option<i32> = None;
    let mut current_kind: Option<ChangeKind> = None;
    let mut saw_deletion = false;

    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        if line.origin() == '-' {
            if let (Some(start), Some(prev), Some(kind)) =
                (current_start, previous_line, current_kind)
            {
                ranges.push(ChangedRange {
                    start_line: start,
                    end_line: prev,
                    kind,
                });
                current_start = None;
                previous_line = None;
                current_kind = None;
            }
            saw_deletion = true;
        } else if line.origin() == '+' {
            if let Some(new_lineno) = line.new_lineno() {
                let new_line = new_lineno as i32;
                match (current_start, previous_line) {
                    (Some(start), Some(prev)) if new_line == prev + 1 => {
                        previous_line = Some(new_line);
                        current_start = Some(start);
                    }
                    _ => {
                        if let (Some(start), Some(prev), Some(kind)) =
                            (current_start, previous_line, current_kind)
                        {
                            ranges.push(ChangedRange {
                                start_line: start,
                                end_line: prev,
                                kind,
                            });
                        }
                        current_start = Some(new_line);
                        previous_line = Some(new_line);
                        current_kind = Some(if saw_deletion {
                            ChangeKind::Modified
                        } else {
                            ChangeKind::Added
                        });
                    }
                }
            }
        } else if current_start.is_some() {
            if let (Some(start), Some(prev), Some(kind)) =
                (current_start, previous_line, current_kind)
            {
                ranges.push(ChangedRange {
                    start_line: start,
                    end_line: prev,
                    kind,
                });
            }
            current_start = None;
            previous_line = None;
            current_kind = None;
            saw_deletion = false;
        }
        true
    })
    .map_err(|e| e.to_string())?;

    if let (Some(start), Some(prev), Some(kind)) = (current_start, previous_line, current_kind) {
        ranges.push(ChangedRange {
            start_line: start,
            end_line: prev,
            kind,
        });
    }

    Ok(ranges)
}

/// Compute rewrite key (hash of normalized patch)
pub fn compute_rewrite_key(repo: &Repository, commit_sha: &str) -> Result<String, String> {
    let oid = Oid::from_str(commit_sha).map_err(|e| e.to_string())?;
    let commit = repo.find_commit(oid).map_err(|e| e.to_string())?;
    let tree = commit.tree().map_err(|e| e.to_string())?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let mut opts = DiffOptions::new();
    opts.context_lines(0);

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))
        .map_err(|e| e.to_string())?;

    let mut hasher = Sha256::new();
    let mut saw_content = false;

    diff.foreach(
        &mut |_delta, _progress| true,
        None,
        None,
        Some(&mut |delta, _hunk, line| {
            let origin = line.origin();
            if origin != '+' && origin != '-' {
                return true;
            }

            let content = std::str::from_utf8(line.content()).unwrap_or_default();
            let trimmed = content.trim_end_matches(&['\n', '\r'][..]);
            if trimmed.starts_with("+++ ") || trimmed.starts_with("--- ") {
                return true;
            }

            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            hasher.update(path.as_bytes());
            hasher.update(b"\n");
            hasher.update([origin as u8]);
            hasher.update(b"\n");
            let normalized = normalize_patch_line(trimmed);
            hasher.update(normalized.as_bytes());
            hasher.update(b"\n");
            saw_content = true;
            true
        }),
    )
    .map_err(|e| e.to_string())?;

    if !saw_content {
        hasher.update(commit.tree_id().to_string().as_bytes());
    }

    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

/// Normalize a patch line (remove whitespace for comparison)
fn normalize_patch_line(line: &str) -> String {
    line.chars().filter(|c| !c.is_whitespace()).collect()
}
