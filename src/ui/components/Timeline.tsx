import { ChevronLeft, ChevronRight } from 'lucide-react';
import { useMemo, useRef } from 'react';
import type { TimelineNode, TimelineBadge } from '../../core/types';

function BadgePill({ badge }: { badge: TimelineBadge }) {
  if (badge.type === 'test') {
    if (badge.status === 'failed') {
      return (
        <span className="pill-test-failed">
          <span className="text-red-500">✕</span>
          {badge.label}
        </span>
      );
    }
    if (badge.status === 'passed') {
      return (
        <span className="pill-test-passed">
          <span className="text-emerald-500">✓</span>
          {badge.label}
        </span>
      );
    }
  }

  if (badge.type === 'trace') {
    return <span className="pill-trace-ai">{badge.label}</span>;
  }

  return <span className="pill-file">{badge.label}</span>;
}

export function Timeline(props: {
  nodes: TimelineNode[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const { nodes, selectedId, onSelect } = props;
  const ref = useRef<HTMLDivElement | null>(null);

  const sorted = useMemo(() => {
    const withTime = nodes.every((n) => !!n.atISO);
    if (!withTime) return nodes;
    return [...nodes].sort((a, b) => String(a.atISO).localeCompare(String(b.atISO)));
  }, [nodes]);

  const scrollBy = (dx: number) => {
    const el = ref.current;
    if (!el) return;
    el.scrollBy({ left: dx, behavior: 'smooth' });
  };

  return (
    <div className="bg-white border-t border-stone-200 px-4 py-4">
      <div className="flex items-center gap-3">
        <button
          type="button"
          className="flex items-center justify-center w-8 h-8 rounded-lg border border-stone-200 bg-stone-50 text-stone-500 hover:bg-stone-100 hover:text-stone-700 active:bg-stone-200 active:scale-95 transition-all"
          onClick={() => scrollBy(-420)}
          aria-label="Scroll timeline left"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>

        <div ref={ref} className="relative flex-1 overflow-x-auto">
          {/* Connection line */}
          <div className="absolute left-0 right-0 top-[19px] h-px bg-stone-200" />
          
          <div className="relative flex min-w-max items-start gap-12 px-4 py-2">
            {sorted.map((n) => {
              const selected = selectedId === n.id;
              const showLabel = n.type === 'milestone' || selected;

              return (
                <div key={n.id} className="relative flex flex-col items-center" style={{ minWidth: '80px' }}>
                  {/* Label above */}
                  {showLabel && n.label ? (
                    <div className="mb-2 w-32 text-center text-[11px] font-medium text-stone-600 leading-tight">
                      {n.label}
                    </div>
                  ) : (
                    <div className="mb-2 h-4" />
                  )}

                  {/* Dot */}
                  <button
                    type="button"
                    className={`timeline-dot transition-all ${n.status || 'ok'} ${selected ? 'selected' : ''}`}
                    onClick={() => onSelect(n.id)}
                    title={n.label ?? n.id}
                    aria-label={n.label ?? n.id}
                  />

                  {/* Badges below dot */}
                  {n.badges && n.badges.length > 0 && (
                    <div className="mt-2 flex flex-col items-center gap-1">
                      {n.badges.slice(0, 2).map((badge, idx) => (
                        <BadgePill key={idx} badge={badge} />
                      ))}
                      {n.badges.length > 2 && (
                        <span className="text-[10px] text-stone-400">+{n.badges.length - 2}</span>
                      )}
                    </div>
                  )}

                  {/* Date below */}
                  <div className="mt-2 h-4 text-[10px] text-stone-400">
                    {showLabel && n.atISO 
                      ? new Date(n.atISO).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })
                      : ''}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        <button
          type="button"
          className="flex items-center justify-center w-8 h-8 rounded-lg border border-stone-200 bg-stone-50 text-stone-500 hover:bg-stone-100 hover:text-stone-700 active:bg-stone-200 active:scale-95 transition-all"
          onClick={() => scrollBy(420)}
          aria-label="Scroll timeline right"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
