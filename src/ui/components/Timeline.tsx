import type { TimelineNode as TimelineNodeType } from '../../core/types';
import { BadgePill } from './BadgePill';
import { TimelineNavButtons } from './TimelineNavButtons';
import { TimelineNodeComponent } from './TimelineNode';
import { useTimelineNavigation } from '../../hooks/useTimelineNavigation';

export interface TimelineProps {
  nodes: TimelineNodeType[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  pulseCommitId?: string | null;
}

export function Timeline({ nodes, selectedId, onSelect, pulseCommitId }: TimelineProps) {
  const {
    containerRef,
    sorted,
    hasPrev,
    hasNext,
    scrollToNode,
  } = useTimelineNavigation({ nodes, selectedId, onSelect });

  return (
    <div className="bg-white border-t border-stone-200 px-4 py-4">
      <div className="flex items-center gap-3">
        <TimelineNavButtons
          hasPrev={hasPrev}
          hasNext={hasNext}
          onPrev={() => scrollToNode('prev')}
          onNext={() => scrollToNode('next')}
        />

        <div
          ref={containerRef}
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
            {sorted.map((n) => (
              <TimelineNodeComponent
                key={n.id}
                node={n}
                selected={selectedId === n.id}
                pulsing={pulseCommitId === n.id}
                onSelect={() => onSelect(n.id)}
              />
            ))}
          </div>
        </div>

        <TimelineNavButtons
          hasPrev={hasPrev}
          hasNext={hasNext}
          onPrev={() => scrollToNode('prev')}
          onNext={() => scrollToNode('next')}
        />
      </div>
    </div>
  );
}

// Re-export for convenience
export { BadgePill };
