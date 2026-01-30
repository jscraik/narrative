import { ChevronLeft, ChevronRight, Link2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { TimelineBadge, TimelineNode } from '../../core/types';
import { AiContributionBadge } from './AiContributionBadge';

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

  if (badge.type === 'contribution' && badge.stats) {
    return (
      <AiContributionBadge
        stats={{
          aiPercentage: badge.stats.aiPercentage,
          primaryTool: badge.stats.tool,
          model: badge.stats.model,
          humanLines: 0,
          aiAgentLines: 0,
          aiAssistLines: 0,
          collaborativeLines: 0,
          totalLines: 0,
        }}
      />
    );
  }

  return <span className="pill-file">{badge.label}</span>;
}

export function Timeline(props: {
  nodes: TimelineNode[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  pulseCommitId?: string | null;
}) {
  const { nodes, selectedId, onSelect, pulseCommitId } = props;
  const ref = useRef<HTMLDivElement | null>(null);
  const reducedMotionQuery = useMemo(
    () => (typeof window !== 'undefined' ? window.matchMedia('(prefers-reduced-motion: reduce)') : null),
    []
  );
  const [_canScrollLeft, setCanScrollLeft] = useState(false);
  const [_canScrollRight, setCanScrollRight] = useState(false);

  const sorted = useMemo(() => {
    const withTime = nodes.every((n) => !!n.atISO);
    if (!withTime) return nodes;
    return [...nodes].sort((a, b) => String(a.atISO).localeCompare(String(b.atISO)));
  }, [nodes]);

  const checkScroll = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const { scrollLeft, scrollWidth, clientWidth } = el;
    setCanScrollLeft(scrollLeft > 1);
    setCanScrollRight(scrollLeft < scrollWidth - clientWidth - 1);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    checkScroll();
    el.addEventListener('scroll', checkScroll, { passive: true });
    window.addEventListener('resize', checkScroll);
    return () => {
      el.removeEventListener('scroll', checkScroll);
      window.removeEventListener('resize', checkScroll);
    };
  }, [checkScroll]);

  const _handleScroll = useCallback((direction: 'left' | 'right') => {
    const el = ref.current;
    if (!el) return;

    // Scroll by ~60% of the visible width or at least 200px
    const scrollAmount = Math.max(200, el.clientWidth * 0.6);
    const targetLeft = direction === 'left'
      ? el.scrollLeft - scrollAmount
      : el.scrollLeft + scrollAmount;

    el.scrollTo({
      left: targetLeft,
      behavior: 'smooth'
    });
  }, []);

  const scrollToNode = useCallback((direction: 'prev' | 'next') => {
    if (!selectedId) return;
    const currentIndex = sorted.findIndex((n) => n.id === selectedId);
    if (currentIndex === -1) return;

    const targetIndex =
      direction === 'prev'
        ? Math.max(0, currentIndex - 1)
        : Math.min(sorted.length - 1, currentIndex + 1);

    if (targetIndex !== currentIndex) {
      onSelect(sorted[targetIndex].id);
    }
  }, [onSelect, selectedId, sorted]);

  // Scroll selected node into view
  useEffect(() => {
    if (!selectedId || !ref.current) return;
    const selectedEl = ref.current.querySelector(`[data-node-id="${selectedId}"]`) as HTMLElement;
    if (selectedEl) {
      const prefersReducedMotion = reducedMotionQuery?.matches ?? false;
      selectedEl.scrollIntoView({
        behavior: prefersReducedMotion ? 'auto' : 'smooth',
        inline: 'center',
        block: 'nearest'
      });
    }
  }, [reducedMotionQuery, selectedId]);

  // Calculate navigation state
  const selectedIndex = useMemo(() => {
    if (!selectedId) return -1;
    return sorted.findIndex((n) => n.id === selectedId);
  }, [sorted, selectedId]);

  const hasPrev = selectedIndex > 0;
  const hasNext = selectedIndex !== -1 && selectedIndex < sorted.length - 1;

  return (
    <div className="bg-white border-t border-stone-200 px-4 py-4">
      <div className="flex items-center gap-3">
        <button
          type="button"
          disabled={!hasPrev}
          className="flex items-center justify-center w-8 h-8 rounded-lg border border-stone-200 bg-stone-50 text-stone-500 hover:bg-stone-100 hover:text-stone-700 active:bg-stone-200 active:scale-95 transition-all disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:bg-stone-50 disabled:hover:text-stone-500"
          onClick={() => scrollToNode('prev')}
          aria-label="Previous commit"
          title="Previous commit (Left Arrow)"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>

        <div
          ref={ref}
          className="relative flex-1 overflow-x-auto no-scrollbar scroll-smooth"
          tabIndex={0}
          role="listbox"
          aria-label="Commit timeline"
          onKeyDown={(event) => {
            if (event.key === 'ArrowLeft') {
              event.preventDefault();
              scrollToNode('prev');
            } else if (event.key === 'ArrowRight') {
              event.preventDefault();
              scrollToNode('next');
            }
          }}
        >
          {/* Connection line - visible path */}
          <div className="absolute left-0 right-0 top-[18px] h-[2px] bg-gradient-to-r from-stone-200 via-stone-300 to-stone-200" />

          <div className="relative flex min-w-max items-start gap-12 px-4 py-2">
            {sorted.map((n) => {
              const selected = selectedId === n.id;
              const pulsing = pulseCommitId === n.id;
              // Always show labels now that we have truncation, to ensure Repo view matches Demo view density
              const showLabel = true;
              const sessionBadge = n.badges?.find(b => b.type === 'session');
              const hasSession = !!sessionBadge;

              return (
                <div
                  key={n.id}
                  data-node-id={n.id}
                  className="relative flex flex-col items-center"
                  style={{ minWidth: '80px' }}
                >
                  {/* Label above */}
                  {showLabel && n.label ? (
                    <div className="mb-2 w-32 text-center text-[11px] font-medium text-stone-600 leading-tight truncate px-1">
                      {n.label}
                    </div>
                  ) : (
                    <div className="mb-2 h-4" />
                  )}

                  {/* Dot with selection glow */}
                  <button
                    type="button"
                    className={`timeline-dot transition-all duration-150 ${n.status || 'ok'} ${selected ? 'selected' : ''} ${hasSession ? 'has-session' : ''} ${pulsing ? 'pulse-once' : ''}`}
                    onClick={() => onSelect(n.id)}
                    title={n.label ?? n.id}
                    aria-label={n.label ?? n.id}
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
                  {n.badges && n.badges.length > 0 && (
                    <div className="mt-2 flex flex-col items-center gap-1">
                      {n.badges
                        .filter(b => b.type !== 'session') // Don't show session badges below
                        .slice(0, 2)
                        .map((badge) => (
                          <BadgePill key={`${badge.type}-${badge.label ?? badge.status ?? 'badge'}`} badge={badge} />
                        ))}
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
          disabled={!hasNext}
          className="flex items-center justify-center w-8 h-8 rounded-lg border border-stone-200 bg-stone-50 text-stone-500 hover:bg-stone-100 hover:text-stone-700 active:bg-stone-200 active:scale-95 transition-all disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:bg-stone-50 disabled:hover:text-stone-500"
          onClick={() => scrollToNode('next')}
          aria-label="Next commit"
          title="Next commit (Right Arrow)"
        >
          <ChevronRight className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
