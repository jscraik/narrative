import type { AttributionPrefs } from '../../core/attribution-api';
import { SourceLensEmptyStates } from './SourceLensEmptyStates';
import { SourceLensLineTable } from './SourceLensLineTable';
import { SourceLensStats } from './SourceLensStats';
import { useSourceLensData } from '../../hooks/useSourceLensData';
import type { SourceLine } from './AuthorBadge';

export interface SourceLensViewProps {
  repoId: number;
  commitSha: string;
  filePath: string;
  prefsOverride?: AttributionPrefs | null;
}

export function SourceLensView({ repoId, commitSha, filePath, prefsOverride }: SourceLensViewProps) {
  const {
    lines,
    stats,
    noteSummary,
    prefs,
    cliStatus,
    loading,
    syncing,
    hasMore,
    error,
    statsError,
    noteSummaryError,
    syncStatus,
    loadMore,
    handleImportNote,
    handleExportNote,
    handleEnableMetadata,
  } = useSourceLensData({ repoId, commitSha, filePath });

  const emptyState = SourceLensEmptyStates({
    loading,
    error,
    linesLength: lines.length,
  });

  if (emptyState) {
    return emptyState;
  }

  const effectivePrefs = prefsOverride ?? prefs;
  const showLineOverlays = effectivePrefs?.showLineOverlays ?? true;

  return (
    <div className="card overflow-hidden">
      <div className="p-5 border-b border-stone-100">
        <SourceLensStats
          lines={lines}
          stats={stats}
          noteSummary={noteSummary}
          prefs={effectivePrefs}
          cliStatus={cliStatus}
          statsError={statsError}
          noteSummaryError={noteSummaryError}
          syncStatus={syncStatus}
          syncing={syncing}
          onImportNote={handleImportNote}
          onExportNote={handleExportNote}
          onEnableMetadata={handleEnableMetadata}
        />
      </div>

      {!showLineOverlays ? (
        <div className="px-5 py-3 text-xs text-stone-400 border-b border-stone-100">
          Line overlays are hidden by preference.
        </div>
      ) : null}

      <SourceLensLineTable lines={lines} showLineOverlays={showLineOverlays} />

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
