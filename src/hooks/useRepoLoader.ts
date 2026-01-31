import { useCallback, useEffect, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { detectCodexOtelPromptExport } from '../core/repo/codexConfig';
import { indexRepo, type IndexingProgress, type RepoIndex } from '../core/repo/indexer';
import { setActiveRepoRoot, setOtelReceiverEnabled } from '../core/tauri/otelReceiver';
import type { BranchViewModel } from '../core/types';
import {
  getAttributionPrefs,
  purgeAttributionPromptMeta,
  setAttributionPrefs,
  type AttributionPrefs,
  type AttributionPrefsUpdate
} from '../core/attribution-api';

export type RepoState =
  | { status: 'idle' }
  | { status: 'loading'; path: string }
  | { status: 'ready'; path: string; model: BranchViewModel; repo: RepoIndex }
  | { status: 'error'; path?: string; message: string };

export interface UseRepoLoaderReturn {
  repoState: RepoState;
  setRepoState: React.Dispatch<React.SetStateAction<RepoState>>;
  indexingProgress: IndexingProgress | null;
  codexPromptExport: { enabled: boolean | null; configPath: string | null };
  attributionPrefs: AttributionPrefs | null;
  actionError: string | null;
  setActionError: (error: string | null) => void;
  openRepo: () => Promise<void>;
  updateAttributionPrefs: (update: AttributionPrefsUpdate) => Promise<void>;
  purgeAttributionMetadata: () => Promise<void>;
  diffCache: React.MutableRefObject<{ clear(): void }>;
}

/**
 * Hook for loading and managing git repository state.
 * Handles repo selection, indexing, and OTLP receiver setup.
 */
export function useRepoLoader(): UseRepoLoaderReturn {
  const [repoState, setRepoState] = useState<RepoState>({ status: 'idle' });
  const [indexingProgress, setIndexingProgress] = useState<IndexingProgress | null>(null);
  const [codexPromptExport, setCodexPromptExport] = useState<{
    enabled: boolean | null;
    configPath: string | null;
  }>({ enabled: null, configPath: null });
  const [attributionPrefs, setAttributionPrefsState] = useState<AttributionPrefs | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  // LRU cache for commit diffs - bounded to prevent memory leaks
  const diffCache = useRef(new Map<string, string>());

  const repoStateRef = useRef(repoState);

  useEffect(() => {
    repoStateRef.current = repoState;
  }, [repoState]);

  useEffect(() => {
    if (repoState.status !== 'ready') {
      setAttributionPrefsState(null);
      return;
    }
    getAttributionPrefs(repoState.repo.repoId)
      .then((prefs) => setAttributionPrefsState(prefs))
      .catch((e) => setActionError(e instanceof Error ? e.message : String(e)));
  }, [repoState]);

  const openRepo = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false, title: 'Select a git repository folder' });
    if (!selected || Array.isArray(selected)) return;

    setRepoState({ status: 'loading', path: selected });
    setIndexingProgress({ phase: 'resolve', message: 'Preparing index…', current: 0, total: 1, percent: 0 });
    setActionError(null);

    try {
      const { model, repo } = await indexRepo(selected, 60, (progress) => {
        setIndexingProgress((prev) => {
          const current = repoStateRef.current;
          if (current.status !== 'loading') return prev;
          return progress;
        });
      });
      setRepoState({ status: 'ready', path: selected, model, repo });
      setIndexingProgress(null);

      // Clear cache when loading a new repo to avoid stale data
      diffCache.current.clear();

      try {
        await setActiveRepoRoot(repo.root);
        const receiverEnabled = model.traceConfig?.codexOtelReceiverEnabled ?? false;
        await setOtelReceiverEnabled(receiverEnabled);
        const promptExport = await detectCodexOtelPromptExport();
        setCodexPromptExport(promptExport);
        const prefs = await getAttributionPrefs(repo.repoId);
        setAttributionPrefsState(prefs);
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    } catch (e: unknown) {
      setRepoState({
        status: 'error',
        path: selected,
        message: e instanceof Error ? e.message : String(e)
      });
      setIndexingProgress(null);
    }
  }, []);

  const updateAttributionPrefs = useCallback(async (update: AttributionPrefsUpdate) => {
    if (repoStateRef.current.status !== 'ready') return;
    try {
      const prefs = await setAttributionPrefs(repoStateRef.current.repo.repoId, update);
      setAttributionPrefsState(prefs);
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const purgeAttributionMetadata = useCallback(async () => {
    if (repoStateRef.current.status !== 'ready') return;
    try {
      await purgeAttributionPromptMeta(repoStateRef.current.repo.repoId);
      const prefs = await getAttributionPrefs(repoStateRef.current.repo.repoId);
      setAttributionPrefsState(prefs);
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  return {
    repoState,
    setRepoState,
    indexingProgress,
    codexPromptExport,
    attributionPrefs,
    actionError,
    setActionError,
    openRepo,
    updateAttributionPrefs,
    purgeAttributionMetadata,
    diffCache: diffCache as unknown as React.MutableRefObject<{ clear(): void }>,
  };
}
