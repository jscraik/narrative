/**
 * Attribution tracking API
 * 
 * Types and functions for AI contribution tracking.
 */

import { invoke } from '@tauri-apps/api/core';

// ============================================================================
// Types
// ============================================================================

export interface ContributionStats {
  humanLines: number;
  aiAgentLines: number;
  aiAssistLines: number;
  collaborativeLines: number;
  totalLines: number;
  aiPercentage: number;
  toolBreakdown?: ToolStats[];
  primaryTool?: string;
  model?: string;
}

export interface ToolStats {
  tool: string;
  model?: string;
  lineCount: number;
}

export interface ImportSuccess {
  path: string;
  sessionId: string;
  warnings: string[];
}

export interface ImportFailure {
  path: string;
  error: string;
  retryable: boolean;
}

export interface BatchImportResult {
  total: number;
  succeeded: ImportSuccess[];
  failed: ImportFailure[];
}

export interface ScannedSession {
  path: string;
  tool: string;
  detectedAt: string;
}

export interface AttributionNoteImportSummary {
  commitSha: string;
  status: string;
  importedRanges: number;
  importedSessions: number;
}

export interface AttributionNoteBatchSummary {
  total: number;
  imported: number;
  missing: number;
  failed: number;
}

export interface AttributionNoteExportSummary {
  commitSha: string;
  status: string;
}

export interface AttributionCoverageSummary {
  totalChangedLines: number;
  attributedLines: number;
  coveragePercent: number;
}

export interface AttributionNoteSummary {
  commitSha: string;
  hasNote: boolean;
  noteRef?: string;
  noteHash?: string;
  schemaVersion?: string;
  metadataAvailable: boolean;
  metadataCached: boolean;
  promptCount?: number;
  coverage?: AttributionCoverageSummary;
  evidenceSource?: string;
}

export interface AttributionPrefs {
  repoId: number;
  cachePromptMetadata: boolean;
  storePromptText: boolean;
  showLineOverlays: boolean;
  retentionDays?: number;
  lastPurgedAt?: string | null;
}

export interface AttributionPrefsUpdate {
  cachePromptMetadata?: boolean;
  storePromptText?: boolean;
  showLineOverlays?: boolean;
  retentionDays?: number;
  clearRetentionDays?: boolean;
}

export interface GitAiCliStatus {
  available: boolean;
  version?: string;
  error?: string;
}

export interface AttributionPromptPurgeSummary {
  removed: number;
}

// ============================================================================
// API Functions
// ============================================================================

/**
 * Scan for available session files in standard locations
 */
export async function scanForSessionFiles(): Promise<ScannedSession[]> {
  return invoke('scan_for_session_files');
}

/**
 * Import a single session file
 */
export async function importSessionFile(
  repoId: number,
  filePath: string
): Promise<BatchImportResult> {
  return invoke('import_session_file', { repoId, filePath });
}

/**
 * Import multiple session files (batch)
 */
export async function importSessionFiles(
  repoId: number,
  filePaths: string[]
): Promise<BatchImportResult> {
  return invoke('import_session_files', { repoId, filePaths });
}

/**
 * Get contribution stats for a commit
 */
export async function getCommitContributionStats(
  repoId: number,
  commitSha: string
): Promise<ContributionStats> {
  return invoke('get_commit_contribution_stats', { repoId, commitSha });
}

/**
 * Compute stats for multiple commits (batch)
 */
export async function computeStatsBatch(
  repoId: number,
  commitShas: string[]
): Promise<number> {
  return invoke('compute_stats_batch', { repoId, commitShas });
}

/**
 * Import a single attribution note (git notes) for a commit.
 */
export async function importAttributionNote(
  repoId: number,
  commitSha: string
): Promise<AttributionNoteImportSummary> {
  return invoke('import_attribution_note', { repoId, commitSha });
}

/**
 * Import attribution notes (git notes) for multiple commits.
 */
export async function importAttributionNotesBatch(
  repoId: number,
  commitShas: string[]
): Promise<AttributionNoteBatchSummary> {
  return invoke('import_attribution_notes_batch', { repoId, commitShas });
}

/**
 * Export local attribution data back into git notes.
 */
export async function exportAttributionNote(
  repoId: number,
  commitSha: string
): Promise<AttributionNoteExportSummary> {
  return invoke('export_attribution_note', { repoId, commitSha });
}

export async function getAttributionNoteSummary(
  repoId: number,
  commitSha: string
): Promise<AttributionNoteSummary> {
  return invoke('get_attribution_note_summary', { repoId, commitSha });
}

export async function getAttributionPrefs(repoId: number): Promise<AttributionPrefs> {
  return invoke('get_attribution_prefs', { repoId });
}

export async function setAttributionPrefs(
  repoId: number,
  update: AttributionPrefsUpdate
): Promise<AttributionPrefs> {
  return invoke('set_attribution_prefs', { repoId, update });
}

export async function purgeAttributionPromptMeta(repoId: number): Promise<AttributionPromptPurgeSummary> {
  return invoke('purge_attribution_prompt_meta', { repoId });
}

export async function getGitAiCliStatus(): Promise<GitAiCliStatus> {
  return invoke('get_git_ai_cli_status');
}

// ============================================================================
// Helpers
// ============================================================================

/**
 * Format AI percentage for display
 */
export function formatAiPercentage(percentage: number): string {
  if (percentage === 0) return '0%';
  if (percentage < 1) return '<1%';
  return `${Math.round(percentage)}%`;
}

/**
 * Get human-readable tool name
 */
export function formatToolName(tool: string): string {
  const toolNames: Record<string, string> = {
    'claude_code': 'Claude',
    'cursor': 'Cursor',
    'copilot': 'Copilot',
    'codex': 'Codex',
    'gemini': 'Gemini',
    'continue': 'Continue',
  };
  
  return toolNames[tool] || tool;
}

/**
 * Get badge style based on AI percentage
 */
export function getBadgeStyle(percentage: number) {
  if (percentage >= 80) {
    return {
      bg: 'bg-emerald-100',
      text: 'text-emerald-700',
      border: 'border-emerald-200',
      icon: 'text-emerald-600',
      label: 'AI',
    };
  } else if (percentage >= 40) {
    return {
      bg: 'bg-amber-100',
      text: 'text-amber-700',
      border: 'border-amber-200',
      icon: 'text-amber-600',
      label: 'Mixed',
    };
  } else if (percentage > 0) {
    return {
      bg: 'bg-blue-100',
      text: 'text-blue-700',
      border: 'border-blue-200',
      icon: 'text-blue-600',
      label: 'Low AI',
    };
  }
  return {
    bg: 'bg-stone-100',
    text: 'text-stone-600',
    border: 'border-stone-200',
    icon: 'text-stone-500',
    label: 'Human',
  };
}
