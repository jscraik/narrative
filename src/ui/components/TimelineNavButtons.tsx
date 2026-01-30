import { ChevronLeft, ChevronRight } from 'lucide-react';

export interface TimelineNavButtonsProps {
  hasPrev: boolean;
  hasNext: boolean;
  onPrev: () => void;
  onNext: () => void;
}

export function TimelineNavButtons({ hasPrev, hasNext, onPrev, onNext }: TimelineNavButtonsProps) {
  return (
    <>
      <button
        type="button"
        disabled={!hasPrev}
        className="flex items-center justify-center w-8 h-8 rounded-lg border border-stone-200 bg-stone-50 text-stone-500 hover:bg-stone-100 hover:text-stone-700 active:bg-stone-200 active:scale-95 transition-all disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:bg-stone-50 disabled:hover:text-stone-500"
        onClick={onPrev}
        aria-label="Previous commit"
        title="Previous commit (Left Arrow)"
      >
        <ChevronLeft className="h-4 w-4" />
      </button>

      <button
        type="button"
        disabled={!hasNext}
        className="flex items-center justify-center w-8 h-8 rounded-lg border border-stone-200 bg-stone-50 text-stone-500 hover:bg-stone-100 hover:text-stone-700 active:bg-stone-200 active:scale-95 transition-all disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:bg-stone-50 disabled:hover:text-stone-500"
        onClick={onNext}
        aria-label="Next commit"
        title="Next commit (Right Arrow)"
      >
        <ChevronRight className="h-4 w-4" />
      </button>
    </>
  );
}
