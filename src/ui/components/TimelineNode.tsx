import { Link2 } from 'lucide-react';
import type { TimelineNode } from '../../core/types';
import { BadgePill } from './BadgePill';

export interface TimelineNodeProps {
  node: TimelineNode;
  selected: boolean;
  pulsing: boolean;
  onSelect: () => void;
}

export function TimelineNodeComponent({ node, selected, pulsing, onSelect }: TimelineNodeProps) {
  // Always show labels now that we have truncation, to ensure Repo view matches Demo view density
  const showLabel = true;
  const sessionBadge = node.badges?.find(b => b.type === 'session');
  const hasSession = !!sessionBadge;

  return (
    <div
      data-node-id={node.id}
      className="relative flex flex-col items-center"
      style={{ minWidth: '80px' }}
    >
      {/* Label above */}
      {showLabel && node.label ? (
        <div className="mb-2 w-32 text-center text-[11px] font-medium text-stone-600 leading-tight truncate px-1">
          {node.label}
        </div>
      ) : (
        <div className="mb-2 h-4" />
      )}

      {/* Dot with selection glow */}
      <button
        type="button"
        className={`timeline-dot transition-all duration-150 ${node.status || 'ok'} ${selected ? 'selected' : ''} ${hasSession ? 'has-session' : ''} ${pulsing ? 'pulse-once' : ''}`}
        onClick={onSelect}
        title={node.label ?? node.id}
        aria-label={node.label ?? node.id}
        aria-current={selected ? 'true' : undefined}
      >
        {/* Session badge overlay on dot */}
        {sessionBadge && (
          <span className="session-badge-overlay">
            <Link2 className="w-2.5 h-2.5" />
          </span>
        )}
      </button>

      {/* Badges below dot */}
      {node.badges && node.badges.length > 0 && (
        <div className="mt-2 flex flex-col items-center gap-1">
          {node.badges
            .filter(b => b.type !== 'session') // Don't show session badges below
            .slice(0, 2)
            .map((badge) => (
              <BadgePill key={`${badge.type}-${badge.label ?? badge.status ?? 'badge'}`} badge={badge} />
            ))}
        </div>
      )}

      {/* Date below */}
      <div className="mt-2 h-4 text-[10px] text-stone-400">
        {showLabel && node.atISO
          ? new Date(node.atISO).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
          : ''}
      </div>
    </div>
  );
}
