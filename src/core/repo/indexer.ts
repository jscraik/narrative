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
import { scanAgentTraceRecords } from './agentTrace';

export type RepoIndex = {
  repoId: number;
  root: string;
  branch: string;
  headSha: string;
};

export async function indexRepo(selectedPath: string, limit = 50): Promise<{ model: BranchViewModel; repo: RepoIndex }> {
  const root = await resolveGitRoot(selectedPath);
  const branch = await getHeadBranch(root);
  const headSha = await getHeadSha(root);

  const repoId = await upsertRepo(root);

  const commits = await listCommits(root, limit);
  await cacheCommitSummaries(repoId, commits);

  const agg = await getAggregateStatsForCommits(root, limit);

  const intent: IntentItem[] = commits.slice(0, 6).map((c, idx) => ({
    id: `c-${idx}`,
    text: c.subject || '(no subject)',
    tag: c.sha.slice(0, 7)
  }));

  const sessionExcerpts = await loadSessionExcerpts(root, 1);
  const trace = await scanAgentTraceRecords(root, repoId, commits.map((c) => c.sha));

  const timeline: TimelineNode[] = commits
    .slice()
    .reverse()
    .map((c) => {
      const traceSummary = trace.byCommit[c.sha];
      const traceBadge = traceSummary
        ? { type: 'trace' as const, label: `AI ${traceSummary.aiPercent}%` }
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

    for (const c of commits) {
      await writeCommitSummaryMeta(root, c);
    }
  } catch {
    // ignore: repo may be read-only, or user may not want any working-tree writes during MVP
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
    traceSummaries: { byCommit: trace.byCommit, byFile: trace.byFile },
    meta: { repoPath: root, branchName: branch, headSha }
  };

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
  } catch {
    // ignore
  }

  return details.fileChanges;
}
