import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import {
  exportAttributionNote,
  getCommitContributionStats,
  importAttributionNote,
  type ContributionStats
} from '../core/attribution-api';
import type { SourceLine } from '../ui/components/AuthorBadge';

const LIMIT = 100;

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
  // Loading states
  loading: boolean;
  syncing: boolean;
  // Pagination
  offset: number;
  hasMore: boolean;
  // Error states
  error: string | null;
  statsError: string | null;
  syncStatus: string | null;
  // Actions
  loadMore: () => void;
  refreshAttribution: () => void;
  handleImportNote: () => Promise<void>;
  handleExportNote: () => Promise<void>;
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

        setLines(result.lines);
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

  useEffect(() => {
    setOffset(0);
    loadAttribution(0);
    loadStats();
  }, [loadAttribution, loadStats]);

  const loadMore = useCallback(() => {
    const nextOffset = offset + LIMIT;
    setOffset(nextOffset);
    loadAttribution(nextOffset);
  }, [offset, loadAttribution]);

  const refreshAttribution = useCallback(() => {
    setOffset(0);
    loadAttribution(0);
    loadStats();
  }, [loadAttribution, loadStats]);

  const handleImportNote = useCallback(async () => {
    setSyncing(true);
    setSyncStatus(null);
    try {
      const summary = await importAttributionNote(repoId, commitSha);
      setSyncStatus(`Imported ${summary.importedRanges} ranges from attribution note.`);
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

  return {
    lines,
    stats,
    loading,
    syncing,
    offset,
    hasMore,
    error,
    statsError,
    syncStatus,
    loadMore,
    refreshAttribution,
    handleImportNote,
    handleExportNote,
  };
}
