import { RefreshCw, Save, HelpCircle } from 'lucide-react';
import { formatToolName } from '../../core/attribution-api';
import type { AttributionNoteSummary, AttributionPrefs, ContributionStats, GitAiCliStatus } from '../../core/attribution-api';
import type { SourceLine } from './AuthorBadge';

export interface SourceLensStatsProps {
  lines: SourceLine[];
  stats: ContributionStats | null;
  noteSummary: AttributionNoteSummary | null;
  prefs: AttributionPrefs | null;
  cliStatus: GitAiCliStatus | null;
  statsError: string | null;
  noteSummaryError: string | null;
  syncStatus: string | null;
  syncing: boolean;
  onImportNote: () => void;
  onExportNote: () => void;
  onEnableMetadata: () => void;
}

export function SourceLensStats({
  lines,
  stats,
  noteSummary,
  prefs,
  cliStatus,
  statsError,
  noteSummaryError,
  syncStatus,
  syncing,
  onImportNote,
  onExportNote,
  onEnableMetadata
}: SourceLensStatsProps) {
  // Calculate stats from current lines
  const agentLines = lines.filter(l => l.authorType === 'ai_agent' || l.authorType === 'ai_tab').length;
  const mixedLines = lines.filter(l => l.authorType === 'mixed').length;
  const humanLines = lines.filter(l => l.authorType === 'human').length;
  const agentPercentage = lines.length > 0
    ? Math.round(((agentLines + mixedLines * 0.5) / lines.length) * 100)
    : 0;
  const hasLocalOnly = lines.some(
    (line) => line.authorType !== 'human' && line.traceAvailable === false
  );
  const metadataAvailable = noteSummary?.metadataAvailable ?? false;
  const metadataCached = noteSummary?.metadataCached ?? false;
  const coveragePercent = noteSummary?.coverage?.coveragePercent;
  const coverageLabel =
    typeof coveragePercent === 'number' ? `${Math.round(coveragePercent)}%` : 'Unknown';
  const evidenceLabel = (() => {
    if (!noteSummary?.hasNote) return 'No notes';
    if (metadataCached) return 'Notes + cached metadata';
    if (metadataAvailable) return 'Notes only (metadata not cached)';
    return 'Notes only (no metadata in note)';
  })();
  const evidenceTitle = (() => {
    if (!noteSummary?.hasNote) return 'No attribution note found for this commit.';
    if (metadataCached) return 'Prompt metadata cached locally.';
    if (metadataAvailable) return 'Metadata is available but not cached yet.';
    return 'Note contains ranges only (no prompt metadata).';
  })();
  const evidenceTone = (() => {
    if (!noteSummary?.hasNote) return 'bg-stone-100 text-stone-500';
    if (metadataCached) return 'bg-emerald-100 text-emerald-700';
    return 'bg-amber-100 text-amber-700';
  })();
  const cacheEnabled = prefs?.cachePromptMetadata ?? false;
  const showMetadataOptIn = metadataAvailable && !metadataCached && !cacheEnabled;
  const showMetadataPending = metadataAvailable && !metadataCached && cacheEnabled;
  const cliLabel = cliStatus?.available
    ? `git-ai CLI detected${cliStatus.version ? ` (${cliStatus.version})` : ''}`
    : 'git-ai CLI not detected';

  return (
    <>
      {/* Header with stats */}
      <div className="flex items-center justify-between">
        <div>
          <div className="section-header">SOURCE LENS</div>
          <div className="section-subheader mt-0.5">line attribution</div>
          {hasLocalOnly ? (
            <div className="mt-2 text-[11px] text-stone-400">
              Session traces are local-only. Import local sessions to view trace details.
            </div>
          ) : null}
          <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-stone-400">
            <span
              className={`inline-flex items-center rounded-full px-2 py-0.5 font-medium ${evidenceTone}`}
              title={evidenceTitle}
            >
              Evidence: {evidenceLabel}
            </span>
            <span>Coverage: {coverageLabel}</span>
            <span>{cliLabel}</span>
          </div>
        </div>
        <div className="flex flex-col items-end gap-2">
          <div className="flex items-center gap-2">
            <div className="w-2 h-2 rounded-full bg-emerald-500" />
            <span className="text-xs text-stone-500">{agentLines} Agent</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-2 h-2 rounded-full bg-amber-500" />
            <span className="text-xs text-stone-500">{mixedLines} Mixed</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-2 h-2 rounded-full bg-stone-300" />
            <span className="text-xs text-stone-500">{humanLines} Human</span>
          </div>
          {mixedLines > 0 ? (
            <div className="text-[11px] text-stone-400 inline-flex items-center gap-1">
              <span>Mixed indicates modified lines (AI + human edits).</span>
              <span title="Legend: added lines are AI, mixed lines are edits.">
                <HelpCircle className="h-3 w-3 text-stone-300" aria-hidden="true" />
              </span>
            </div>
          ) : null}
          <div className="mt-2 flex items-center gap-2">
            <button
              type="button"
              onClick={onImportNote}
              disabled={syncing}
              className="inline-flex items-center gap-1 rounded-md border border-stone-200 bg-white px-2 py-1 text-[11px] font-medium text-stone-600 transition-colors motion-reduce:transition-none hover:bg-stone-50 disabled:opacity-50"
            >
              <RefreshCw className={`h-3 w-3 ${syncing ? 'motion-safe:animate-spin' : ''}`} />
              Import note
            </button>
            <button
              type="button"
              onClick={onExportNote}
              disabled={syncing}
              className="inline-flex items-center gap-1 rounded-md border border-stone-200 bg-white px-2 py-1 text-[11px] font-medium text-stone-600 transition-colors motion-reduce:transition-none hover:bg-stone-50 disabled:opacity-50"
            >
              <Save className="h-3 w-3" />
              Export note
            </button>
          </div>
        </div>
      </div>

      {/* Stats bar */}
      <div className="mt-4 flex h-2 rounded-full overflow-hidden">
        {agentLines > 0 && (
          <div
            className="bg-emerald-500"
            style={{ width: `${(agentLines / lines.length) * 100}%` }}
          />
        )}
        {mixedLines > 0 && (
          <div
            className="bg-amber-500"
            style={{ width: `${(mixedLines / lines.length) * 100}%` }}
          />
        )}
        {humanLines > 0 && (
          <div
            className="bg-stone-300"
            style={{ width: `${(humanLines / lines.length) * 100}%` }}
          />
        )}
      </div>
      <div className="mt-1 text-xs text-stone-500 text-right">
        {agentPercentage}% agent-generated
      </div>
      {stats?.toolBreakdown && stats.toolBreakdown.length > 0 ? (
        <div className="mt-2 text-[11px] text-stone-500 text-right">
          Tools:{' '}
          {stats.toolBreakdown.slice(0, 2).map((toolStat, index) => (
            <span key={`${toolStat.tool}-${toolStat.model ?? 'unknown'}`}>
              {index > 0 ? ' · ' : ''}
              {formatToolName(toolStat.tool)} {toolStat.lineCount}
            </span>
          ))}
        </div>
      ) : null}
      {noteSummary?.promptCount ? (
        <div className="mt-1 text-[11px] text-stone-400 text-right">
          Prompts: {noteSummary.promptCount}
        </div>
      ) : null}
      {showMetadataPending ? (
        <div className="mt-3 rounded-md border border-amber-100 bg-amber-50 px-3 py-2 text-[11px] text-amber-700">
          <div className="font-semibold">Prompt metadata caching is enabled.</div>
          <div className="mt-1">Re-import the attribution note to cache prompt summaries.</div>
        </div>
      ) : null}
      {showMetadataOptIn ? (
        <div className="mt-3 rounded-md border border-sky-100 bg-sky-50 px-3 py-2 text-[11px] text-sky-700">
          <div className="font-semibold">Prompt metadata is available for this repo.</div>
          <div className="mt-1">Enable caching to view prompt summaries and tool/model details.</div>
          <button
            type="button"
            onClick={onEnableMetadata}
            disabled={syncing}
            className="mt-2 inline-flex items-center gap-1 rounded-md border border-sky-200 bg-white px-2 py-1 text-[11px] font-semibold text-sky-700 hover:bg-sky-100 disabled:opacity-50"
          >
            Enable metadata
          </button>
        </div>
      ) : null}
      {syncStatus ? (
        <div className="mt-2 text-[11px] text-stone-400 text-right">{syncStatus}</div>
      ) : null}
      {statsError ? (
        <div className="mt-2 text-[11px] text-amber-600 text-right">{statsError}</div>
      ) : null}
      {noteSummaryError ? (
        <div className="mt-2 text-[11px] text-amber-600 text-right">{noteSummaryError}</div>
      ) : null}
      <div className="mt-3 text-[11px] text-stone-400">
        Attribution indicates how lines were generated or edited; it is not a legal ownership claim.
      </div>
    </>
  );
}
