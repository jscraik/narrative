/**
 * Rules-only reviewer system
 *
 * Provides user-defined rules for code review with:
 * - No default checks (only user-defined rules)
 * - Quiet on pass (no output if all rules pass)
 * - Non-zero exit on violations
 */

import { invoke } from '@tauri-apps/api/core';

/**
 * Rule severity level
 */
export type RuleSeverity = 'error' | 'warning';

/**
 * Rule definition
 */
export interface Rule {
  /** Unique rule identifier */
  name: string;
  /** Human-readable description */
  description: string;
  /** Regex pattern to match (or simple string for contains check) */
  pattern: string;
  /** Is this a regex pattern? */
  is_regex?: boolean;
  /** Severity level */
  severity?: RuleSeverity;
  /** File patterns to include (glob-style) */
  include_files?: string[];
  /** File patterns to exclude (glob-style) */
  exclude_files?: string[];
  /** Suggested fix message */
  suggestion?: string;
}

/**
 * Rule violation result
 */
export interface RuleViolation {
  /** Name of the rule that was violated */
  rule_name: string;
  /** Severity of the violation */
  severity: RuleSeverity;
  /** File where violation occurred */
  file: string;
  /** Line number where violation occurred */
  line: number;
  /** The matched content */
  matched: string;
  /** Suggested fix */
  suggestion: string;
}

/**
 * Review result summary
 */
export interface ReviewSummary {
  total_files_scanned: number;
  total_rules: number;
  violations_found: number;
  errors: number;
  warnings: number;
}

/**
 * Complete review result
 */
export interface ReviewResult {
  summary: ReviewSummary;
  violations: RuleViolation[];
  files_scanned: string[];
  rules_applied: string[];
}

/**
 * Rule validation error
 */
export interface RuleValidationError {
  rule_name: string;
  error: string;
}

/**
 * Review a repository against rules
 *
 * Returns JSON output with violations found.
 * If no violations, returns empty violations array.
 *
 * @param repoRoot - Path to the repository root
 * @returns Review result with violations and summary
 */
export async function reviewRepo(repoRoot: string): Promise<ReviewResult> {
  return invoke('review_repo', { repoRoot });
}

/**
 * Get all loaded rules for a repository
 *
 * @param repoRoot - Path to the repository root
 * @returns Array of rules
 */
export async function getRules(repoRoot: string): Promise<Rule[]> {
  return invoke('get_rules', { repoRoot });
}

/**
 * Validate a rule set JSON file
 *
 * @param repoRoot - Path to the repository root
 * @param ruleFile - Name of the rule file (relative to .narrative/rules/ or absolute path)
 * @returns Array of validation errors (empty if valid)
 */
export async function validateRules(
  repoRoot: string,
  ruleFile: string
): Promise<RuleValidationError[]> {
  return invoke('validate_rules', { repoRoot, ruleFile });
}

/**
 * Create a default rule set template
 *
 * Creates `.narrative/rules/default.json` with example rules.
 *
 * @param repoRoot - Path to the repository root
 * @returns Success message with path to created file
 */
export async function createDefaultRules(repoRoot: string): Promise<string> {
  return invoke('create_default_rules', { repoRoot });
}

/**
 * Run review and return appropriate exit code
 *
 * This is a convenience function that:
 * - Returns 0 if no violations (quiet on pass)
 * - Returns 1 if violations found (outputs JSON)
 * - Returns 2 if validation errors
 *
 * Note: In a browser/Tauri context, this returns the exit code
 * rather than actually exiting the process.
 *
 * @param repoRoot - Path to the repository root
 * @returns Exit code (0 = success, 1 = violations, 2 = errors)
 */
export async function reviewRepoWithExitCode(repoRoot: string): Promise<{
  exitCode: number;
  result: ReviewResult;
}> {
  const result = await reviewRepo(repoRoot);

  let exitCode = 0;
  if (result.summary.violations_found > 0) {
    exitCode = 1;
  }

  return { exitCode, result };
}
