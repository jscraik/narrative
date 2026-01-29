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

export type SessionTool = 'claude-code' | 'codex' | 'unknown';

export type SessionMessageRole = 'user' | 'assistant';

export type SessionMessage = {
  id: string;
  role: SessionMessageRole;
  text: string;
  files?: string[];
};

export type SessionExcerpt = {
  id: string;
  tool: SessionTool;
  durationMin?: number;
  messages: SessionMessage[];
};

export type TimelineStatus = 'ok' | 'warn' | 'error';

export type TimelineBadge = {
  type: 'file' | 'test' | 'trace';
  label: string;
  status?: 'passed' | 'failed' | 'mixed';
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
    byFile: Record<string, TraceFileSummary>;
  };
  meta?: {
    repoPath?: string;
    branchName?: string;
    headSha?: string;
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
