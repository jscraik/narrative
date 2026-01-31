//! Coverage computation for attribution notes

use super::git_utils::collect_changed_ranges_by_file;
use super::line_attribution::fetch_line_attributions_for_commit;
use super::utils::fetch_repo_root;
use git2::Repository;
use std::collections::HashMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionCoverageSummary {
    pub total_changed_lines: u32,
    pub attributed_lines: u32,
    pub coverage_percent: f32,
}

pub async fn compute_attribution_coverage(
    db: &sqlx::SqlitePool,
    repo_id: i64,
    commit_sha: &str,
) -> Result<Option<AttributionCoverageSummary>, String> {
    let repo_root = fetch_repo_root(db, repo_id).await?;
    let repo = Repository::open(&repo_root).map_err(|e| e.to_string())?;

    let attributions = fetch_line_attributions_for_commit(db, repo_id, commit_sha).await?;
    if attributions.is_empty() {
        return Ok(None);
    }

    let mut by_file: HashMap<String, Vec<(i32, i32)>> = HashMap::new();
    for row in attributions {
        by_file
            .entry(row.file_path)
            .or_default()
            .push((row.start_line, row.end_line));
    }

    let mut total_changed_lines: u32 = 0;
    let mut attributed_lines: u32 = 0;

    let changed_by_file = collect_changed_ranges_by_file(&repo, commit_sha)?;
    for (file_path, changed_ranges) in changed_by_file {
        if changed_ranges.is_empty() {
            continue;
        }

        let mut changed: Vec<(i32, i32)> = changed_ranges
            .into_iter()
            .map(|range| (range.start_line, range.end_line))
            .collect();
        changed = merge_ranges(changed);

        total_changed_lines += sum_ranges(&changed);

        if let Some(attr_ranges) = by_file.get(&file_path) {
            let merged_attr = merge_ranges(attr_ranges.clone());
            attributed_lines += count_intersection(&changed, &merged_attr);
        }
    }

    if total_changed_lines == 0 {
        return Ok(None);
    }

    let coverage_percent = (attributed_lines as f32 / total_changed_lines as f32) * 100.0;

    Ok(Some(AttributionCoverageSummary {
        total_changed_lines,
        attributed_lines,
        coverage_percent,
    }))
}

fn merge_ranges(mut ranges: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: Vec<(i32, i32)> = Vec::new();
    for (start, end) in ranges {
        let normalized_end = end.max(start);
        if let Some(last) = merged.last_mut() {
            if start <= last.1 + 1 {
                last.1 = last.1.max(normalized_end);
                continue;
            }
        }
        merged.push((start, normalized_end));
    }
    merged
}

fn sum_ranges(ranges: &[(i32, i32)]) -> u32 {
    ranges
        .iter()
        .map(|(start, end)| (end - start + 1).max(0) as u32)
        .sum()
}

fn count_intersection(changed: &[(i32, i32)], attributed: &[(i32, i32)]) -> u32 {
    let mut i = 0;
    let mut j = 0;
    let mut count = 0;

    while i < changed.len() && j < attributed.len() {
        let (c_start, c_end) = changed[i];
        let (a_start, a_end) = attributed[j];

        if c_end < a_start {
            i += 1;
            continue;
        }
        if a_end < c_start {
            j += 1;
            continue;
        }

        let start = c_start.max(a_start);
        let end = c_end.min(a_end);
        if end >= start {
            count += (end - start + 1) as u32;
        }

        if c_end < a_end {
            i += 1;
        } else {
            j += 1;
        }
    }

    count
}
