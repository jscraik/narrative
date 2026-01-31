import { HelpCircle } from 'lucide-react';

export interface SourceLensEmptyStatesProps {
  loading: boolean;
  error: string | null;
  linesLength: number;
}

export function SourceLensEmptyStates({ loading, error, linesLength }: SourceLensEmptyStatesProps) {
  if (loading && linesLength === 0) {
    return (
      <div className="card p-5">
        <div className="section-header">SOURCE LENS</div>
        <div className="mt-4 flex items-center gap-2 text-sm text-stone-500">
          <div className="w-4 h-4 border-2 border-stone-300 border-t-sky-500 rounded-full motion-safe:animate-spin motion-reduce:animate-none" />
          Loading source lens...
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="card p-5">
        <div className="section-header">SOURCE LENS</div>
        <div className="mt-4 text-sm text-red-600">{error}</div>
      </div>
    );
  }

  if (linesLength === 0) {
    return (
      <div className="card p-5">
        <div className="section-header">SOURCE LENS</div>
        <div className="mt-4 flex flex-col items-center text-center py-4">
          <div className="w-12 h-12 rounded-full bg-stone-100 flex items-center justify-center mb-3">
            <HelpCircle className="w-5 h-5 text-stone-400" />
          </div>
          <p className="text-sm text-stone-500 mb-1">No attribution data</p>
          <p className="text-xs text-stone-400">
            Import a session or attribution note to see line sources
          </p>
          <p className="mt-2 text-[11px] text-stone-400">
            Attribution is sourced from git notes; no AI detection is performed.
          </p>
        </div>
      </div>
    );
  }

  return null;
}
