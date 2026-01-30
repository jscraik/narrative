import type { SourceLine } from './AuthorBadge';
import { SourceLensEmptyStates } from './SourceLensEmptyStates';
import { SourceLensLineTable } from './SourceLensLineTable';
import { SourceLensStats } from './SourceLensStats';
import { useSourceLensData } from '../../hooks/useSourceLensData';

export interface SourceLensViewProps {
  repoId: number;
  commitSha: string;
  filePath: string;
}

export function SourceLensView({ repoId, commitSha, filePath }: SourceLensViewProps) {
  const {
    lines,
    stats,
    loading,
    syncing,
    hasMore,
    error,
    statsError,
    syncStatus,
    loadMore,
    handleImportNote,
    handleExportNote,
  } = useSourceLensData({ repoId, commitSha, filePath });

  const emptyState = SourceLensEmptyStates({
    loading,
    error,
    linesLength: lines.length,
  });

  if (emptyState) {
    return emptyState;
  }

  return (
    <div className="card overflow-hidden">
      <div className="p-5 border-b border-stone-100">
        <SourceLensStats
          lines={lines}
          stats={stats}
          statsError={statsError}
          syncStatus={syncStatus}
          syncing={syncing}
          onImportNote={handleImportNote}
          onExportNote={handleExportNote}
        />
      </div>

      <SourceLensLineTable lines={lines} />

      {hasMore && (
        <div className="p-3 border-t border-stone-100 text-center">
          <button
            type="button"
            onClick={loadMore}
            className="text-xs text-sky-600 hover:text-sky-700 font-medium"
          >
            Load more...
          </button>
        </div>
      )}
    </div>
  );
}

// Re-export types for convenience
export type { SourceLine };
