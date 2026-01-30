import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { TimelineNode } from '../core/types';

export interface UseTimelineNavigationProps {
  nodes: TimelineNode[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export interface UseTimelineNavigationReturn {
  // Ref for the scroll container
  containerRef: React.RefObject<HTMLDivElement>;
  // Sorted nodes
  sorted: TimelineNode[];
  // Navigation state
  hasPrev: boolean;
  hasNext: boolean;
  // Scroll state
  canScrollLeft: boolean;
  canScrollRight: boolean;
  // Actions
  scrollToNode: (direction: 'prev' | 'next') => void;
  handleScroll: (direction: 'left' | 'right') => void;
}

export function useTimelineNavigation({
  nodes,
  selectedId,
  onSelect
}: UseTimelineNavigationProps): UseTimelineNavigationReturn {
  const containerRef = useRef<HTMLDivElement>(null);
  const reducedMotionQuery = useMemo(
    () => (typeof window !== 'undefined' ? window.matchMedia('(prefers-reduced-motion: reduce)') : null),
    []
  );
  const [canScrollLeft, setCanScrollLeft] = useState(false);
  const [canScrollRight, setCanScrollRight] = useState(false);

  const sorted = useMemo(() => {
    const withTime = nodes.every((n) => !!n.atISO);
    if (!withTime) return nodes;
    return [...nodes].sort((a, b) => String(a.atISO).localeCompare(String(b.atISO)));
  }, [nodes]);

  const checkScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    const { scrollLeft, scrollWidth, clientWidth } = el;
    setCanScrollLeft(scrollLeft > 1);
    setCanScrollRight(scrollLeft < scrollWidth - clientWidth - 1);
  }, []);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    checkScroll();
    el.addEventListener('scroll', checkScroll, { passive: true });
    window.addEventListener('resize', checkScroll);
    return () => {
      el.removeEventListener('scroll', checkScroll);
      window.removeEventListener('resize', checkScroll);
    };
  }, [checkScroll]);

  const handleScroll = useCallback((direction: 'left' | 'right') => {
    const el = containerRef.current;
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
    if (!selectedId || !containerRef.current) return;
    const selectedEl = containerRef.current.querySelector(`[data-node-id="${selectedId}"]`) as HTMLElement;
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

  return {
    containerRef,
    sorted,
    hasPrev,
    hasNext,
    canScrollLeft,
    canScrollRight,
    scrollToNode,
    handleScroll,
  };
}
