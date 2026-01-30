import { useRef, useEffect } from 'react';
import { FileCode } from 'lucide-react';
import type { FileChange, TraceFileSummary } from '../../core/types';
import { useFileSelection } from '../../core/context/FileSelectionContext';

function formatDelta(n: number) {
  const sign = n >= 0 ? '+' : '';
  return `${sign}${n}`;
}

export function FilesChanged({
  files,
  title,
  traceByFile
}: {
  files: FileChange[];
  title?: string;
  traceByFile?: Record<string, TraceFileSummary>;
}) {
  const { selectedFile, selectFile } = useFileSelection();
  const fileRefs = useRef<Map<string, HTMLButtonElement>>(new Map());

  // Scroll selected file into view
  useEffect(() => {
    if (selectedFile) {
      const el = fileRefs.current.get(selectedFile);
      if (el) {
        el.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }
    }
  }, [selectedFile]);

  return (
    <div className="card p-5">
      <div className="section-header">{title ?? 'FILES CHANGED'}</div>
      <div className="mt-4 divide-y divide-stone-100 border border-stone-100 rounded-lg overflow-hidden">
        {files.length === 0 ? (
          <div className="p-6 flex flex-col items-center text-center">
            <div className="w-10 h-10 rounded-full bg-stone-100 flex items-center justify-center mb-2">
              <FileCode className="w-4 h-4 text-stone-400" />
            </div>
            <p className="text-sm text-stone-500">No files changed</p>
            <p className="text-xs text-stone-400 mt-0.5">Select a commit to view changes</p>
          </div>
        ) : (
          files.map((f) => (
            <button
              key={f.path}
              ref={(el) => {
                if (el) fileRefs.current.set(f.path, el);
              }}
              type="button"
              aria-pressed={selectedFile === f.path}
              className={`flex w-full items-center justify-between gap-3 px-4 py-3 text-left text-sm transition-all ${
                selectedFile === f.path 
                  ? 'bg-sky-50 border-l-2 border-l-sky-500 -ml-[2px] pl-[18px]' 
                  : 'hover:bg-stone-50 border-l-2 border-l-transparent'
              }`}
              onClick={() => selectFile(f.path)}
            >
              <div className={`truncate font-mono text-[12px] ${
                selectedFile === f.path ? 'text-sky-700' : 'text-stone-600'
              }`}>
                {f.path}
              </div>
              <div className="flex shrink-0 items-center gap-2 font-mono text-[11px] tabular-nums">
                {traceByFile?.[f.path] ? (
                  (() => {
                    const summary = traceByFile[f.path];
                    // Show "Unknown" pill if only unknown lines exist (no AI/human/mixed)
                    const isUnknownOnly =
                      summary.unknownLines > 0 &&
                      summary.aiLines === 0 &&
                      summary.humanLines === 0 &&
                      summary.mixedLines === 0;

                    return isUnknownOnly ? (
                      <span className="pill-trace-unknown">Unknown</span>
                    ) : (
                      <span className="pill-trace-ai">AI {summary.aiPercent}%</span>
                    );
                  })()
                ) : null}
                <span className="text-emerald-600">{formatDelta(f.additions)}</span>
                <span className="text-red-500">-{f.deletions}</span>
              </div>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
