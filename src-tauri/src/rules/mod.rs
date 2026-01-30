//! Rules-only reviewer system
//!
//! Provides user-defined rules for code review with:
//! - No default checks (only user-defined rules)
//! - Quiet on pass (no output if all rules pass)
//! - Non-zero exit on violations

pub mod commands;

use serde::{Deserialize, Serialize};

/// Rule definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Unique rule identifier
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// Regex pattern to match (or simple string for contains check)
    #[serde(default)]
    pub pattern: String,
    /// Is this a regex pattern? (default: false = simple contains)
    #[serde(default)]
    pub is_regex: bool,
    /// Severity level: "error" or "warning"
    #[serde(default = "default_severity")]
    pub severity: RuleSeverity,
    /// File patterns to include (glob-style)
    #[serde(default)]
    pub include_files: Vec<String>,
    /// File patterns to exclude (glob-style)
    #[serde(default)]
    pub exclude_files: Vec<String>,
    /// Suggested fix message
    #[serde(default)]
    pub suggestion: String,
}

fn default_severity() -> RuleSeverity {
    RuleSeverity::Error
}

/// Rule severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSeverity {
    Error,
    Warning,
}

/// Rule set (collection of rules)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    pub name: String,
    pub version: String,
    pub rules: Vec<Rule>,
}

/// Rule violation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleViolation {
    /// Name of the rule that was violated
    pub rule_name: String,
    /// Severity of the violation
    pub severity: RuleSeverity,
    /// File where violation occurred
    pub file: String,
    /// Line number where violation occurred (0 if unknown)
    pub line: usize,
    /// The matched content
    pub matched: String,
    /// Suggested fix
    pub suggestion: String,
}

/// Review result summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSummary {
    pub total_files_scanned: usize,
    pub total_rules: usize,
    pub violations_found: usize,
    pub errors: usize,
    pub warnings: usize,
}

/// Complete review result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    pub summary: ReviewSummary,
    pub violations: Vec<RuleViolation>,
    pub files_scanned: Vec<String>,
    pub rules_applied: Vec<String>,
}

/// Rule validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleValidationError {
    pub rule_name: String,
    pub error: String,
}
