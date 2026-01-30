import { RefreshCw, Save, HelpCircle } from 'lucide-react';
import { formatToolName } from '../../core/attribution-api';
import type { ContributionStats } from '../../core/attribution-api';
import type { SourceLine } from './AuthorBadge';

export interface SourceLensStatsProps {
  lines: SourceLine[];
  stats: ContributionStats | null;
  statsError: string | null;
  syncStatus: string | null;
  syncing: boolean;
  onImportNote: () => void;
  onExportNote: () => void;
}

export function SourceLensStats({
  lines,
  stats,
  statsError,
  syncStatus,
  syncing,
  onImportNote,
  onExportNote
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
              className="inline-flex items-center gap-1 rounded-md border border-stone-200 bg-white px-2 py-1 text-[11px] font-medium text-stone-600 transition-colors hover:bg-stone-50 disabled:opacity-50"
            >
              <RefreshCw className={`h-3 w-3 ${syncing ? 'animate-spin' : ''}`} />
              Import note
            </button>
            <button
              type="button"
              onClick={onExportNote}
              disabled={syncing}
              className="inline-flex items-center gap-1 rounded-md border border-stone-200 bg-white px-2 py-1 text-[11px] font-medium text-stone-600 transition-colors hover:bg-stone-50 disabled:opacity-50"
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
      {syncStatus ? (
        <div className="mt-2 text-[11px] text-stone-400 text-right">{syncStatus}</div>
      ) : null}
      {statsError ? (
        <div className="mt-2 text-[11px] text-amber-600 text-right">{statsError}</div>
      ) : null}
    </>
  );
}
