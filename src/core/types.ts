export type BranchStatus = 'open' | 'merged';

export type Stats = {
  added: number;
  removed: number;
  files: number;
  commits: number;
  prompts: number;
  responses: number;
};

export type IntentItem = {
  id: string;
  text: string;
  tag?: string;
};

export type FileChange = {
  path: string;
  additions: number;
  deletions: number;
};

export type SessionTool =
  | 'claude-code'
  | 'codex'
  | 'kimi'
  | 'cursor'
  | 'gemini'
  | 'copilot'
  | 'continue'
  | 'unknown';

export type SessionMessageRole = 'user' | 'assistant' | 'thinking' | 'plan' | 'tool_call';

export type SessionMessage = {
  id: string;
  role: SessionMessageRole;
  text: string;
  files?: string[];
  toolName?: string;
  toolInput?: unknown;
};

export type SessionExcerpt = {
  id: string;
  tool: SessionTool;
  durationMin?: number;
  messages: SessionMessage[];
  // Link state (Phase 1 MVP)
  linkedCommitSha?: string;
  linkConfidence?: number;
  autoLinked?: boolean;
};

export type TimelineStatus = 'ok' | 'warn' | 'error';

export type TimelineBadge = {
  type: 'file' | 'test' | 'trace' | 'contribution' | 'session';
  label: string;
  status?: 'passed' | 'failed' | 'mixed';
  stats?: {
    aiPercentage: number;
    tool?: string;
    model?: string;
  };
};

export type TraceContributorType = 'human' | 'ai' | 'mixed' | 'unknown';

export type TraceContributor = {
  type: TraceContributorType;
  modelId?: string;
};

export type TraceRange = {
  startLine: number;
  endLine: number;
  contentHash?: string;
  contributor?: TraceContributor;
};

export type TraceConversation = {
  url?: string;
  contributor?: TraceContributor;
  ranges: TraceRange[];
  related?: Array<{ type: string; url: string }>;
};

export type TraceFile = {
  path: string;
  conversations: TraceConversation[];
};

export type TraceRecord = {
  id: string;
  version: string;
  timestamp: string;
  vcs: { type: 'git'; revision: string };
  tool?: { name?: string; version?: string };
  files: TraceFile[];
  metadata?: Record<string, unknown>;
};

export type TraceFileSummary = {
  path: string;
  aiLines: number;
  humanLines: number;
  mixedLines: number;
  unknownLines: number;
  aiPercent: number;
};

export type TraceCommitSummary = {
  commitSha: string;
  aiLines: number;
  humanLines: number;
  mixedLines: number;
  unknownLines: number;
  aiPercent: number;
  modelIds: string[];
};

export type TraceCollectorStatus = {
  state: 'active' | 'inactive' | 'error' | 'partial';
  message?: string;
  issues?: string[];
  lastSeenAtISO?: string;
};

export type TraceCollectorConfig = {
  codexOtelLogPath: string;
  codexOtelReceiverEnabled: boolean;
};

export type TimelineNode = {
  id: string;
  atISO?: string;
  label?: string;
  status?: TimelineStatus;
  type: 'milestone' | 'commit';
  badges?: TimelineBadge[];
  testRunId?: string;
};

export type BranchViewModel = {
  source: 'demo' | 'git';
  title: string;
  status: BranchStatus;
  description: string;
  stats: Stats;
  intent: IntentItem[];
  timeline: TimelineNode[];
  // Optional, mainly for demo mode
  sessionExcerpts?: SessionExcerpt[];
  filesChanged?: FileChange[];
  diffsByFile?: Record<string, string>;
  traceSummaries?: {
    byCommit: Record<string, TraceCommitSummary>;
    byFileByCommit: Record<string, Record<string, TraceFileSummary>>;
  };
  traceStatus?: TraceCollectorStatus;
  traceConfig?: TraceCollectorConfig;
  meta?: {
    repoPath?: string;
    branchName?: string;
    headSha?: string;
    repoId?: number;
  };
};

export type CommitSummary = {
  sha: string;
  subject: string;
  author: string;
  authoredAtISO: string;
};

export type CommitDetails = {
  sha: string;
  fileChanges: FileChange[];
};

export type TestStatus = 'passed' | 'failed' | 'skipped';

export type TestCase = {
  id: string;
  name: string;
  status: TestStatus;
  durationMs: number;
  errorMessage?: string;
  filePath?: string;
};

export type TestRun = {
  id: string;
  sessionId?: string;
  commitSha?: string;
  atISO: string;
  durationSec: number;
  passed: number;
  failed: number;
  skipped: number;
  tests: TestCase[];
};

// EnhancedTimelineNode is now just TimelineNode (badges and testRunId added above)
export type EnhancedTimelineNode = TimelineNode;

// ============================================================================
// Rules System Types
// ============================================================================

export type RuleSeverity = 'error' | 'warning';

export type Rule = {
  name: string;
  description: string;
  pattern: string;
  is_regex?: boolean;
  severity?: RuleSeverity;
  include_files?: string[];
  exclude_files?: string[];
  suggestion?: string;
};

export type RuleViolation = {
  rule_name: string;
  severity: RuleSeverity;
  file: string;
  line: number;
  matched: string;
  suggestion: string;
};

export type ReviewSummary = {
  total_files_scanned: number;
  total_rules: number;
  violations_found: number;
  errors: number;
  warnings: number;
};

export type ReviewResult = {
  summary: ReviewSummary;
  violations: RuleViolation[];
  files_scanned: string[];
  rules_applied: string[];
};

export type RuleValidationError = {
  rule_name: string;
  error: string;
};
