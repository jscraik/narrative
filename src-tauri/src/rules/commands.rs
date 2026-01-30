//! Tauri commands for rules-only reviewer

use super::{ReviewResult, Rule, RuleSet, RuleSeverity, RuleValidationError};
use regex::Regex;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Load rules from a rule set JSON file
fn load_rules_from_json(path: &Path) -> Result<RuleSet, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read rules file {}: {}", path.display(), e))?;

    let rule_set: RuleSet = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse rules JSON: {}", e))?;

    Ok(rule_set)
}

/// Load all rule sets from the rules directory
fn load_all_rules(repo_root: &Path) -> Result<Vec<Rule>, String> {
    let rules_dir = repo_root.join(".narrative/rules");

    if !rules_dir.exists() {
        // No rules directory means no rules to check
        return Ok(vec![]);
    }

    let mut all_rules = vec![];

    let entries = fs::read_dir(&rules_dir)
        .map_err(|e| format!("Failed to read rules directory {}: {}", rules_dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();

        // Only process .json files
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }

        // Skip schema.json
        if path.file_name().and_then(|s| s.to_str()) == Some("schema.json") {
            continue;
        }

        match load_rules_from_json(&path) {
            Ok(rule_set) => {
                all_rules.extend(rule_set.rules);
            }
            Err(e) => {
                eprintln!("Warning: Failed to load rules from {}: {}", path.display(), e);
            }
        }
    }

    Ok(all_rules)
}

/// Check if a file matches the include/exclude patterns
fn file_matches_patterns(file_path: &str, include: &[String], exclude: &[String]) -> bool {
    // If no include patterns, include all files
    let include_file = if include.is_empty() {
        true
    } else {
        include.iter().any(|pattern| {
            glob_match(pattern, file_path)
        })
    };

    if !include_file {
        return false;
    }

    // Check exclude patterns
    !exclude.iter().any(|pattern| glob_match(pattern, file_path))
}

/// Simple glob pattern matching (supports * and ** wildcards)
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern_regex = pattern
        .replace('.', "\\.")
        .replace("**", ".*")
        .replace('*', "[^/]*")
        .replace('?', ".");

    match Regex::new(&format!("^{}$", pattern_regex)) {
        Ok(re) => re.is_match(text),
        Err(_) => false,
    }
}

/// Find line number for a match in a file
fn find_match_line(content: &str, matched: &str) -> usize {
    let byte_offset = content.find(matched).unwrap_or(0);
    let line_num = content[..byte_offset].lines().count() + 1;
    line_num
}

/// Scan a single file for rule violations
fn scan_file_for_violations(
    file_path: &Path,
    repo_root: &Path,
    rules: &[Rule],
) -> Vec<super::RuleViolation> {
    let mut violations = vec![];

    let relative_path = file_path
        .strip_prefix(repo_root)
        .ok()
        .and_then(|p| p.to_str())
        .unwrap_or(file_path.to_str().unwrap_or(""))
        .replace('\\', "/");

    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return violations, // Skip files we can't read
    };

    for rule in rules {
        // Check if file matches include/exclude patterns
        if !file_matches_patterns(&relative_path, &rule.include_files, &rule.exclude_files) {
            continue;
        }

        let matches: Vec<(usize, &str)> = if rule.is_regex {
            match Regex::new(&rule.pattern) {
                Ok(re) => re
                    .find_iter(&content)
                    .map(|m| (m.start(), m.as_str()))
                    .collect(),
                Err(_) => continue, // Invalid regex, skip this rule
            }
        } else {
            content
                .match_indices(&rule.pattern)
                .map(|(idx, m)| (idx, m))
                .collect()
        };

        for (_, matched) in matches {
            let line = find_match_line(&content, matched);
            violations.push(super::RuleViolation {
                rule_name: rule.name.clone(),
                severity: rule.severity,
                file: relative_path.clone(),
                line,
                matched: matched.to_string(),
                suggestion: rule.suggestion.clone(),
            });
        }
    }

    violations
}

/// Recursively find all source files in the repo
fn find_source_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut files = vec
![];

    fn visit_dir(dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();

                // Skip .narrative, .git, node_modules, target, etc.
                if path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| {
                        s == ".narrative"
                            || s == ".git"
                            || s == "node_modules"
                            || s == "target"
                            || s == "dist"
                            || s == "build"
                    })
                    .unwrap_or(false)
                {
                    continue;
                }

                if path.is_dir() {
                    visit_dir(&path, files);
                } else if path.is_file() {
                    // Only include source files with common extensions
                    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                        const SOURCE_EXTENSIONS: &[&str] = &[
                            "rs", "ts", "tsx", "js", "jsx", "py", "go", "java", "c", "cpp", "h",
                            "hpp", "cs", "swift", "kt", "rb", "php", "sh", "sql", "md", "json",
                            "yaml", "yml", "toml",
                        ];
                        if SOURCE_EXTENSIONS.contains(&ext) {
                            files.push(path);
                        }
                    }
                }
            }
        }
    }

    visit_dir(repo_root, &mut files);
    files
}

/// Review a repository against rules
#[tauri::command(rename_all = "camelCase")]
pub async fn review_repo(repo_root: String) -> Result<ReviewResult, String> {
    let repo_path = PathBuf::from(&repo_root);

    if !repo_path.exists() {
        return Err(format!("Repository path does not exist: {}", repo_root));
    }

    // Load all rules
    let rules = load_all_rules(&repo_path)?;

    if rules.is_empty() {
        // No rules configured
        return Ok(ReviewResult {
            summary: super::ReviewSummary {
                total_files_scanned: 0,
                total_rules: 0,
                violations_found: 0,
                errors: 0,
                warnings: 0,
            },
            violations: vec![],
            files_scanned: vec![],
            rules_applied: vec![],
        });
    }

    // Find all source files
    let files = find_source_files(&repo_path);

    // Scan each file for violations
    let mut all_violations = vec![];

    for file in &files {
        let file_violations = scan_file_for_violations(file, &repo_path, &rules);
        all_violations.extend(file_violations);
    }

    // Count errors and warnings
    let errors = all_violations
        .iter()
        .filter(|v| v.severity == RuleSeverity::Error)
        .count();
    let warnings = all_violations
        .iter()
        .filter(|v| v.severity == RuleSeverity::Warning)
        .count();

    // Build file list
    let files_scanned: Vec<String> = files
        .iter()
        .filter_map(|p| {
            p.strip_prefix(&repo_path)
                .ok()
                .and_then(|p| p.to_str())
                .map(|s| s.replace('\\', "/"))
        })
        .collect();

    // Build rule name list
    let rules_applied: Vec<String> = rules.iter().map(|r| r.name.clone()).collect();

    Ok(ReviewResult {
        summary: super::ReviewSummary {
            total_files_scanned: files_scanned.len(),
            total_rules: rules.len(),
            violations_found: all_violations.len(),
            errors,
            warnings,
        },
        violations: all_violations,
        files_scanned,
        rules_applied,
    })
}

/// Get all loaded rules for a repository
#[tauri::command(rename_all = "camelCase")]
pub async fn get_rules(repo_root: String) -> Result<Vec<Rule>, String> {
    let repo_path = PathBuf::from(&repo_root);
    let rules = load_all_rules(&repo_path)?;
    Ok(rules)
}

/// Validate a rule set JSON file
#[tauri::command(rename_all = "camelCase")]
pub async fn validate_rules(repo_root: String, rule_file: String) -> Result<Vec<RuleValidationError>, String> {
    let repo_path = PathBuf::from(&repo_root);
    let rules_path = if PathBuf::from(&rule_file).is_absolute() {
        PathBuf::from(rule_file)
    } else {
        repo_path.join(".narrative/rules").join(&rule_file)
    };

    let mut errors = vec![];

    match load_rules_from_json(&rules_path) {
        Ok(rule_set) => {
            // Validate each rule
            for rule in &rule_set.rules {
                // Check if regex pattern is valid
                if rule.is_regex {
                    if let Err(e) = Regex::new(&rule.pattern) {
                        errors.push(super::RuleValidationError {
                            rule_name: rule.name.clone(),
                            error: format!("Invalid regex pattern: {}", e),
                        });
                    }
                }

                // Check if rule has a name
                if rule.name.is_empty() {
                    errors.push(super::RuleValidationError {
                        rule_name: rule.name.clone(),
                        error: "Rule name cannot be empty".into(),
                    });
                }

                // Check if rule has a pattern
                if rule.pattern.is_empty() {
                    errors.push(super::RuleValidationError {
                        rule_name: rule.name.clone(),
                        error: "Rule pattern cannot be empty".into(),
                    });
                }
            }
        }
        Err(e) => {
            errors.push(super::RuleValidationError {
                rule_name: String::from("<file>"),
                error: format!("Failed to parse rule file: {}", e),
            });
        }
    }

    Ok(errors)
}

/// Create a default rule set template
#[tauri::command(rename_all = "camelCase")]
pub async fn create_default_rules(repo_root: String) -> Result<String, String> {
    let repo_path = PathBuf::from(&repo_root);
    let rules_dir = repo_path.join(".narrative/rules");

    fs::create_dir_all(&rules_dir)
        .map_err(|e| format!("Failed to create rules directory: {}", e))?;

    let default_rules_path = rules_dir.join("default.json");
    let default_rules = r#"{
  "name": "Default Rules",
  "version": "1.0.0",
  "rules": [
    {
      "name": "no-todo",
      "description": "Flag TODO comments (should be tracked in issue tracker)",
      "pattern": "TODO:",
      "is_regex": false,
      "severity": "warning",
      "include_files": ["**/*.rs", "**/*.ts", "**/*.tsx", "**/*.js"],
      "exclude_files": [],
      "suggestion": "Create an issue in your tracker instead of using TODO comments"
    },
    {
      "name": "no-console-log",
      "description": "Flag console.log statements (use proper logging)",
      "pattern": "console\\.log\\(",
      "is_regex": true,
      "severity": "warning",
      "include_files": ["**/*.ts", "**/*.tsx", "**/*.js"],
      "exclude_files": [],
      "suggestion": "Use a proper logging library instead of console.log"
    }
  ]
}"#;

    if default_rules_path.exists() {
        return Err("Default rules already exist".into());
    }

    fs::write(&default_rules_path, default_rules)
        .map_err(|e| format!("Failed to write default rules: {}", e))?;

    Ok(format!("Created default rules at {}", default_rules_path.display()))
}
