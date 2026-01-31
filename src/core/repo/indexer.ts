import type { BranchViewModel, IntentItem, TimelineNode } from '../types';
import { cacheCommitSummaries, cacheFileChanges, getCachedFileChanges, upsertRepo } from './db';
import { getAggregateStatsForCommits, getCommitDetails, getHeadBranch, getHeadSha, listCommits, resolveGitRoot } from './git';
import {
  branchStatsPayload,
  ensureRepoNarrativeLayout,
  writeBranchMeta,
  writeCommitFilesMeta,
  writeCommitSummaryMeta,
  writeRepoMeta
} from './meta';
import { loadSessionExcerpts } from './sessions';
import { ingestCodexOtelLogFile, scanAgentTraceRecords } from './agentTrace';
import { loadTraceConfig } from './traceConfig';
import { importAttributionNotesBatch } from '../attribution-api';

export type IndexingProgress = {
  phase: string;
  message: string;
  current?: number;
  total?: number;
  percent?: number;
};

export type RepoIndex = {
  repoId: number;
  root: string;
  branch: string;
  headSha: string;
};

export async function indexRepo(
  selectedPath: string,
  limit = 50,
  onProgress?: (progress: IndexingProgress) => void
): Promise<{ model: BranchViewModel; repo: RepoIndex }> {
  const phaseOrder = [
    'resolve',
    'branch',
    'repo',
    'commits',
    'summaries',
    'stats',
    'notes',
    'intent',
    'sessions',
    'trace-config',
    'trace',
    'meta',
    'done'
  ];

  const reportProgress = (phase: string, message: string, current?: number, total?: number) => {
    if (!onProgress) return;
    const phaseIndex = phaseOrder.indexOf(phase);
    const segment = phaseIndex >= 0 ? 100 / (phaseOrder.length - 1) : undefined;
    const ratio = typeof total === 'number' && total > 0 && typeof current === 'number'
      ? Math.min(1, current / total)
      : 0;
    const percent = segment !== undefined
      ? Math.min(100, Math.round(segment * phaseIndex + ratio * segment))
      : undefined;
    onProgress({ phase, message, current, total, percent });
  };

  const yieldToMain = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

  reportProgress('resolve', 'Resolving repository…');
  const root = await resolveGitRoot(selectedPath);
  reportProgress('branch', 'Reading branch metadata…');
  const branch = await getHeadBranch(root);
  const headSha = await getHeadSha(root);

  reportProgress('repo', 'Preparing repo index…');
  const repoId = await upsertRepo(root);

  reportProgress('commits', 'Listing commits…');
  const commits = await listCommits(root, limit);
  reportProgress('summaries', 'Caching commit summaries…', 0, commits.length);
  const cachePromise = cacheCommitSummaries(repoId, commits, (current, total) => {
    reportProgress('summaries', 'Caching commit summaries…', current, total);
  });

  reportProgress('stats', 'Computing aggregate stats…');
  const [_, agg] = await Promise.all([
    cachePromise,
    getAggregateStatsForCommits(root, limit)
  ]);

  reportProgress('notes', 'Importing attribution notes…');
  try {
    await importAttributionNotesBatch(repoId, commits.map((c) => c.sha));
  } catch (e) {
    // Notes import is best-effort, but log the error for debugging
    console.error('[Indexer] Attribution notes import failed:', e);
  }

  reportProgress('intent', 'Preparing intent summaries…');
  const intent: IntentItem[] = commits.slice(0, 6).map((c, idx) => ({
    id: `c-${idx}`,
    text: c.subject || '(no subject)',
    tag: c.sha.slice(0, 7)
  }));

  reportProgress('sessions', 'Loading session excerpts…');
  const sessionExcerpts = await loadSessionExcerpts(root, repoId, 1);
  reportProgress('trace-config', 'Loading trace configuration…');
  const traceConfig = await loadTraceConfig(root);
  
  // Trace ingestion is best-effort; don't fail repo loading if it errors
  let otelIngest: Awaited<ReturnType<typeof ingestCodexOtelLogFile>> = {
    status: { state: 'inactive', message: 'Not configured' },
    recordsWritten: 0
  };
  let trace: Awaited<ReturnType<typeof scanAgentTraceRecords>> = {
    byCommit: {},
    byFileByCommit: {},
    totals: { conversations: 0, ranges: 0 }
  };
  
  reportProgress('trace', 'Scanning trace data…');
  try {
    otelIngest = await ingestCodexOtelLogFile({
      repoRoot: root,
      repoId,
      logPath: traceConfig.codexOtelLogPath
    });
    trace = await scanAgentTraceRecords(root, repoId, commits.map((c) => c.sha));
  } catch (e) {
    // Trace scanning failed, but we can still show the repo without trace data
    console.error('[Indexer] Trace scanning failed:', e);
  }

  const timeline: TimelineNode[] = commits
    .slice()
    .reverse()
    .map((c) => {
      const traceSummary = trace.byCommit[c.sha];
      const traceBadge = traceSummary
        ? { type: 'trace' as const, label: getTraceBadgeLabel(traceSummary) }
        : null;

      return {
        id: c.sha,
        type: 'commit',
        label: c.subject,
        atISO: c.authoredAtISO,
        status: 'ok',
        badges: traceBadge ? [traceBadge] : undefined
      };
    });

  const stats = {
    added: agg.added,
    removed: agg.removed,
    files: agg.uniqueFiles,
    commits: commits.length,
    prompts: trace.totals.conversations,
    responses: trace.totals.ranges
  };

  reportProgress('meta', 'Writing metadata snapshots…', 0, commits.length);
  // Best-effort: write shareable meta snapshots into `.narrative/meta/**`
  try {
    await ensureRepoNarrativeLayout(root);

    await writeRepoMeta(root, {
      repoRoot: root,
      indexedAtISO: new Date().toISOString(),
      commitLimit: limit
    });

    await writeBranchMeta(
      root,
      branch,
      branchStatsPayload({
        repoRoot: root,
        branch,
        headSha,
        stats,
        commitShas: commits.map((c) => c.sha)
      })
    );

    for (let index = 0; index < commits.length; index += 1) {
      const c = commits[index];
      await writeCommitSummaryMeta(root, c);
      const current = index + 1;
      if (current % 20 === 0 || current === commits.length) {
        reportProgress('meta', 'Writing metadata snapshots…', current, commits.length);
      }
      if (current % 50 === 0) {
        await yieldToMain();
      }
    }
  } catch (e) {
    // Repo may be read-only, or user may not want any working-tree writes during MVP
    console.warn('[Indexer] Metadata write failed (repo may be read-only):', e);
  }

  const model: BranchViewModel = {
    source: 'git',
    title: branch,
    status: 'open',
    description: root,
    stats,
    intent,
    timeline,
    sessionExcerpts,
    traceSummaries: { byCommit: trace.byCommit, byFileByCommit: trace.byFileByCommit },
    traceStatus: otelIngest.status,
    traceConfig,
    meta: { repoPath: root, branchName: branch, headSha, repoId }
  };

  reportProgress('done', 'Index complete', commits.length, commits.length);
  return { model, repo: { repoId, root, branch, headSha } };
}

export async function getOrLoadCommitFiles(repo: RepoIndex, sha: string) {
  const cached = await getCachedFileChanges(repo.repoId, sha);
  if (cached) return cached;

  const details = await getCommitDetails(repo.root, sha);
  await cacheFileChanges(repo.repoId, details);

  // Best-effort: write committable metadata for this commit's file list.
  try {
    await writeCommitFilesMeta(repo.root, sha, details.fileChanges);
  } catch (e) {
    // Metadata write may fail (read-only fs, etc.), but file loading still works
    console.warn('[Indexer] Commit files metadata write failed:', e);
  }

  return details.fileChanges;
}

// Helper function to determine trace badge label
function getTraceBadgeLabel(summary: { aiLines: number; humanLines: number; mixedLines: number; unknownLines: number; aiPercent: number }): string {
  const isUnknownOnly =
    summary.unknownLines > 0 &&
    summary.aiLines === 0 &&
    summary.humanLines === 0 &&
    summary.mixedLines === 0;

  return isUnknownOnly ? 'Unknown' : `AI ${summary.aiPercent}%`;
}
