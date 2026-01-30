import { useCallback, useEffect, useMemo, useState } from 'react';
import { getCommitDiffForFile } from '../core/repo/git';
import { getOrLoadCommitFiles } from '../core/repo/indexer';
import { getTraceRangesForCommitFile } from '../core/repo/agentTrace';
import type { BranchViewModel, FileChange, TraceRange } from '../core/types';
import type { RepoState } from './useRepoLoader';

// Lazy-load demo model to avoid bundling issues
let demoModelCache: BranchViewModel | null = null;
let demoModelPromise: Promise<BranchViewModel> | null = null;

async function loadDemoModel(): Promise<BranchViewModel> {
  if (demoModelCache) return demoModelCache;
  if (demoModelPromise) return demoModelPromise;

  demoModelPromise = (async () => {
    const mod = await import('../core/demo/nearbyGridDemo');
    demoModelCache = mod.NearbyGridDemo;
    return mod.NearbyGridDemo;
  })();

  return demoModelPromise;
}

export interface UseCommitDataProps {
  mode: 'demo' | 'repo' | 'speculate' | 'docs';
  repoState: RepoState;
  diffCache: React.MutableRefObject<{ get(key: string): string | undefined; set(key: string, value: string): void }>;
  model: BranchViewModel | null;
}

export interface UseCommitDataReturn {
  model: BranchViewModel | null;
  repoPath: string | null;
  loadFilesForNode: (nodeId: string) => Promise<FileChange[]>;
  loadDiffForFile: (nodeId: string, filePath: string) => Promise<string>;
  loadTraceRangesForFile: (nodeId: string, filePath: string) => Promise<TraceRange[]>;
}

/**
 * Hook for loading commit-related data (files, diffs, traces).
 * Provides memoized model/path values and cached diff loading.
 */
export function useCommitData({
  mode,
  repoState,
  diffCache,
  model: _model
}: UseCommitDataProps): UseCommitDataReturn {
  const [demoModel, setDemoModel] = useState<BranchViewModel | null>(null);

  // Load demo model asynchronously when entering demo mode
  useEffect(() => {
    if (mode === 'demo' && !demoModel) {
      loadDemoModel().then(setDemoModel);
    }
  }, [mode, demoModel]);

  const computedModel = useMemo(() => {
    if (mode === 'demo') {
      return demoModel;
    }
    if (mode === 'repo' && repoState.status === 'ready') return repoState.model;
    return null;
  }, [mode, repoState, demoModel]);

  const repoPath = useMemo(() => {
    if (repoState.status === 'ready') return repoState.repo.root;
    if (repoState.status === 'loading') return repoState.path;
    return null;
  }, [repoState]);

  const loadFilesForNode = useCallback(
    async (nodeId: string): Promise<FileChange[]> => {
      if (!computedModel) return [];

      if (computedModel.source === 'demo') {
        return computedModel.filesChanged ?? [];
      }

      if (repoState.status !== 'ready') return [];
      return await getOrLoadCommitFiles(repoState.repo, nodeId);
    },
    [computedModel, repoState]
  );

  const loadDiffForFile = useCallback(
    async (nodeId: string, filePath: string): Promise<string> => {
      if (!computedModel) return '';

      if (computedModel.source === 'demo') {
        return computedModel.diffsByFile?.[filePath] ?? '(no demo diff for this file)';
      }

      if (repoState.status !== 'ready') return '';

      const cacheKey = `${nodeId}:${filePath}`;
      const cached = diffCache.current.get(cacheKey);
      if (cached) return cached;

      const diff = await getCommitDiffForFile(repoState.repo.root, nodeId, filePath);
      diffCache.current.set(cacheKey, diff);
      return diff;
    },
    [computedModel, repoState, diffCache]
  );

  const loadTraceRangesForFile = useCallback(
    async (nodeId: string, filePath: string) => {
      if (!computedModel) return [];
      if (computedModel.source === 'demo') return [];
      if (repoState.status !== 'ready') return [];
      return await getTraceRangesForCommitFile(repoState.repo.repoId, nodeId, filePath);
    },
    [computedModel, repoState]
  );

  return {
    model: computedModel,
    repoPath,
    loadFilesForNode,
    loadDiffForFile,
    loadTraceRangesForFile
  };
}
