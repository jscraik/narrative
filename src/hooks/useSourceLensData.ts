import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  exportAttributionNote,
  getCommitContributionStats,
  getAttributionNoteSummary,
  getAttributionPrefs,
  getGitAiCliStatus,
  importAttributionNote,
  setAttributionPrefs,
  type ContributionStats
} from '../core/attribution-api';
import type { AttributionNoteSummary, AttributionPrefs, GitAiCliStatus } from '../core/attribution-api';
import type { SourceLine } from '../ui/components/AuthorBadge';

const LIMIT = 200; // Balance between UX (context) and render cost for large files.

export interface SourceLensResult {
  lines: SourceLine[];
  totalLines: number;
  hasMore: boolean;
}

export interface UseSourceLensDataProps {
  repoId: number;
  commitSha: string;
  filePath: string;
}

export interface UseSourceLensDataReturn {
  // Data
  lines: SourceLine[];
  stats: ContributionStats | null;
  noteSummary: AttributionNoteSummary | null;
  prefs: AttributionPrefs | null;
  cliStatus: GitAiCliStatus | null;
  // Loading states
  loading: boolean;
  syncing: boolean;
  // Pagination
  offset: number;
  hasMore: boolean;
  // Error states
  error: string | null;
  statsError: string | null;
  noteSummaryError: string | null;
  syncStatus: string | null;
  // Actions
  loadMore: () => void;
  refreshAttribution: () => void;
  handleImportNote: () => Promise<void>;
  handleExportNote: () => Promise<void>;
  handleEnableMetadata: () => Promise<void>;
}

export function useSourceLensData({
  repoId,
  commitSha,
  filePath
}: UseSourceLensDataProps): UseSourceLensDataReturn {
  const [lines, setLines] = useState<SourceLine[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [stats, setStats] = useState<ContributionStats | null>(null);
  const [statsError, setStatsError] = useState<string | null>(null);
  const [noteSummary, setNoteSummary] = useState<AttributionNoteSummary | null>(null);
  const [noteSummaryError, setNoteSummaryError] = useState<string | null>(null);
  const [prefs, setPrefs] = useState<AttributionPrefs | null>(null);
  const [cliStatus, setCliStatus] = useState<GitAiCliStatus | null>(null);
  const [syncStatus, setSyncStatus] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  const loadAttribution = useCallback(
    async (requestedOffset: number) => {
      setLoading(true);
      setError(null);

      try {
        const result = await invoke<SourceLensResult>('get_file_source_lens', {
          request: {
            repoId,
            commitSha,
            filePath,
            offset: requestedOffset,
            limit: LIMIT,
          },
        });

        setLines((previous) =>
          requestedOffset === 0 ? result.lines : [...previous, ...result.lines]
        );
        setHasMore(result.hasMore);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    [commitSha, filePath, repoId]
  );

  const loadStats = useCallback(async () => {
    setStatsError(null);
    try {
      const result = await getCommitContributionStats(repoId, commitSha);
      setStats(result);
    } catch (e) {
      setStatsError(e instanceof Error ? e.message : String(e));
    }
  }, [commitSha, repoId]);

  const loadNoteSummary = useCallback(async () => {
    setNoteSummaryError(null);
    try {
      const summary = await getAttributionNoteSummary(repoId, commitSha);
      setNoteSummary(summary);
    } catch (e) {
      setNoteSummaryError(e instanceof Error ? e.message : String(e));
    }
  }, [commitSha, repoId]);

  const loadPrefs = useCallback(async () => {
    try {
      const result = await getAttributionPrefs(repoId);
      setPrefs(result);
    } catch (e) {
      setSyncStatus(e instanceof Error ? e.message : String(e));
    }
  }, [repoId]);

  const loadCliStatus = useCallback(async () => {
    try {
      const result = await getGitAiCliStatus();
      setCliStatus(result);
    } catch {
      setCliStatus({ available: false });
    }
  }, []);

  useEffect(() => {
    setOffset(0);
    loadAttribution(0);
    loadStats();
    loadNoteSummary();
    loadPrefs();
    loadCliStatus();
  }, [loadAttribution, loadStats, loadNoteSummary, loadPrefs, loadCliStatus]);

  const loadMore = useCallback(() => {
    const nextOffset = offset + LIMIT;
    setOffset(nextOffset);
    loadAttribution(nextOffset);
  }, [offset, loadAttribution]);

  const refreshAttribution = useCallback(() => {
    setOffset(0);
    loadAttribution(0);
    loadStats();
    loadNoteSummary();
  }, [loadAttribution, loadStats, loadNoteSummary]);

  const handleImportNote = useCallback(async () => {
    setSyncing(true);
    setSyncStatus(null);
    try {
      const summary = await importAttributionNote(repoId, commitSha);
      if (summary.status === 'missing') {
        setSyncStatus('No attribution note found for this commit.');
      } else if (summary.status === 'invalid') {
        setSyncStatus('Attribution note was empty or invalid.');
      } else {
        setSyncStatus(`Imported ${summary.importedRanges} ranges from attribution note.`);
      }
      refreshAttribution();
    } catch (e) {
      setSyncStatus(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(false);
    }
  }, [commitSha, refreshAttribution, repoId]);

  const handleExportNote = useCallback(async () => {
    setSyncing(true);
    setSyncStatus(null);
    try {
      await exportAttributionNote(repoId, commitSha);
      setSyncStatus('Exported attribution note to git notes.');
    } catch (e) {
      setSyncStatus(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(false);
    }
  }, [commitSha, repoId]);

  const handleEnableMetadata = useCallback(async () => {
    setSyncing(true);
    setSyncStatus(null);
    try {
      await setAttributionPrefs(repoId, { cachePromptMetadata: true });
      await importAttributionNote(repoId, commitSha);
      await loadPrefs();
      await loadNoteSummary();
      setSyncStatus('Enabled prompt metadata caching for this repo.');
      refreshAttribution();
    } catch (e) {
      setSyncStatus(e instanceof Error ? e.message : String(e));
    } finally {
      setSyncing(false);
    }
  }, [commitSha, loadNoteSummary, loadPrefs, refreshAttribution, repoId]);

  return {
    lines,
    stats,
    noteSummary,
    prefs,
    cliStatus,
    loading,
    syncing,
    offset,
    hasMore,
    error,
    statsError,
    noteSummaryError,
    syncStatus,
    loadMore,
    refreshAttribution,
    handleImportNote,
    handleExportNote,
    handleEnableMetadata,
  };
}
