import { useMemo } from 'react';
import { Activity, Sparkles } from 'lucide-react';
import type { TraceCollectorStatus, TraceCommitSummary } from '../../core/types';

function StatPill({ label, value, tone }: { label: string; value: string; tone: 'ai' | 'human' | 'mixed' | 'unknown' }) {
  const className =
    tone === 'ai'
      ? 'pill-trace-ai'
      : tone === 'human'
        ? 'pill-trace-human'
        : tone === 'mixed'
          ? 'pill-trace-mixed'
          : 'pill-trace-unknown';

  return (
    <span className={`inline-flex items-center gap-2 ${className}`}>
      <span className="text-[10px] uppercase tracking-wide">{label}</span>
      <span className="font-semibold tabular-nums">{value}</span>
    </span>
  );
}

export function AgentTraceSummary(props: {
  summary?: TraceCommitSummary;
  onExport?: () => void;
  onSmokeTest?: () => void;
  hasFiles: boolean;
  status?: TraceCollectorStatus;
}) {
  const { summary, onExport, onSmokeTest, hasFiles, status } = props;
  const aiPercent = summary?.aiPercent ?? 0;
  const activeWindowMs = 5 * 60 * 1000;
  const statusSummary = useMemo(() => {
    const lastSeenAt = status?.lastSeenAtISO ? Date.parse(status.lastSeenAtISO) : null;
    const lastSeenAgeMs = lastSeenAt ? Math.max(0, Date.now() - lastSeenAt) : null;
    const isStale = Boolean(
      lastSeenAgeMs !== null && status?.state === 'active' && lastSeenAgeMs > activeWindowMs
    );
    const displayState = status
      ? isStale
        ? 'inactive'
        : status.state
      : undefined;
    const lastSeenLabel =
      lastSeenAgeMs !== null
        ? lastSeenAgeMs < 60_000
          ? `Last seen ${Math.max(1, Math.round(lastSeenAgeMs / 1000))}s ago`
          : `Last seen ${Math.round(lastSeenAgeMs / 60_000)}m ago`
        : null;
    return { displayState, lastSeenLabel, isStale };
  }, [status]);
  const { displayState, lastSeenLabel, isStale } = statusSummary;
  const statusTooltip = displayState
    ? `Active if Codex OTel events are seen within 5 minutes.${lastSeenLabel ? ` ${lastSeenLabel}.` : ''}${
        isStale ? ' Last activity is outside the 5-minute window.' : ''
      }`
    : undefined;

  return (
    <div className="card p-5">
      <div className="flex items-center justify-between">
        <div>
          <div className="section-header">AI ATTRIBUTION</div>
          <div className="section-subheader mt-0.5">AI vs human contributions</div>
        </div>
        <div className="flex items-center gap-2">
          {status ? (
            <span
              className={
                displayState === 'active'
                  ? 'pill-trace-ai'
                  : displayState === 'inactive'
                    ? 'pill-trace-unknown'
                    : displayState === 'partial'
                      ? 'pill-trace-mixed'
                      : 'pill-trace-mixed'
              }
              title={statusTooltip}
            >
              {displayState === 'active'
                ? 'Codex OTel: Active'
                : displayState === 'inactive'
                  ? 'Codex OTel: Inactive'
                  : displayState === 'partial'
                    ? 'Codex OTel: Partial'
                    : 'Codex OTel: Error'}
            </span>
          ) : null}
          {onSmokeTest ? (
            <button
              type="button"
              className="inline-flex items-center gap-2 rounded-lg border border-stone-200 bg-white px-3 py-1.5 text-xs font-semibold text-stone-600 transition-all duration-150 hover:bg-stone-50 disabled:cursor-not-allowed disabled:opacity-50"
              onClick={onSmokeTest}
              disabled={!hasFiles}
              aria-disabled={!hasFiles}
            >
              <Sparkles className="h-3.5 w-3.5" />
              Run Codex OTel Smoke Test
            </button>
          ) : null}
          {onExport ? (
            <button
              type="button"
              className="inline-flex items-center gap-2 rounded-lg border border-stone-200 bg-white px-3 py-1.5 text-xs font-semibold text-stone-600 hover:bg-stone-50 disabled:cursor-not-allowed disabled:opacity-50"
              onClick={onExport}
              disabled={!hasFiles}
              aria-disabled={!hasFiles}
            >
              Export Agent Trace
            </button>
          ) : null}
        </div>
      </div>

      {status?.message ? (
        <div className="mt-4 text-xs text-stone-500">{status.message}</div>
      ) : null}
      {lastSeenLabel ? <div className="mt-1 text-xs text-stone-400">{lastSeenLabel}</div> : null}
      {status?.issues && status.issues.length > 0 ? (
        <div className="mt-2 text-xs text-stone-400">
          {status.issues.slice(0, 3).map((issue) => (
            <div key={issue}>• {issue}</div>
          ))}
          {status.issues.length > 3 ? <div>+{status.issues.length - 3} more</div> : null}
        </div>
      ) : null}

      {!summary ? (
        <div className="mt-5 rounded-xl border border-dashed border-stone-200 bg-stone-50/50 px-5 py-6">
          <div className="flex flex-col items-center text-center">
            <div className="w-12 h-12 rounded-full bg-stone-100 flex items-center justify-center mb-3">
              <Activity className="w-5 h-5 text-stone-400" />
            </div>
            <p className="text-sm font-medium text-stone-600 mb-1">No Agent Trace yet</p>
            <p className="text-xs text-stone-400 max-w-[240px] leading-relaxed">
              Import a session or configure Codex OTel to see AI attribution for this commit
            </p>
            <div className="mt-3 flex items-center gap-1.5 text-[11px] text-stone-400">
              <Sparkles className="w-3 h-3" />
              <span>Tracks AI vs human contributions</span>
            </div>
          </div>
        </div>
      ) : (
        <div className="mt-4 space-y-4">
          <div className="flex items-center gap-3">
            <div className="trace-bar">
              <div className="trace-bar-fill" style={{ width: `${aiPercent}%` }} />
            </div>
            <div className="text-sm font-semibold text-stone-700">AI {aiPercent}%</div>
          </div>

          <div className="flex flex-wrap gap-2">
            <StatPill label="AI" value={`${summary.aiLines}`} tone="ai" />
            <StatPill label="Human" value={`${summary.humanLines}`} tone="human" />
            <StatPill label="Mixed" value={`${summary.mixedLines}`} tone="mixed" />
            <StatPill label="Unknown" value={`${summary.unknownLines}`} tone="unknown" />
          </div>

          <div className="text-xs text-stone-500 leading-relaxed">
            Agent Trace highlights which lines were generated by AI versus edited by humans. It is a
            helpful guide, not a legal ownership claim.
          </div>

          {summary.modelIds.length > 0 ? (
            <div className="text-xs text-stone-400">
              Models: {summary.modelIds.join(', ')}
            </div>
          ) : (
            <div className="text-xs text-stone-400">Models: unknown</div>
          )}
        </div>
      )}
    </div>
  );
}
