import type { TraceRange, TraceContributorType } from '../../core/types';

function lineClass(line: string) {
  if (line.startsWith('@@')) return 'diff-line-hunk';
  if (line.startsWith('+') && !line.startsWith('+++')) return 'diff-line-add';
  if (line.startsWith('-') && !line.startsWith('---')) return 'diff-line-del';
  return '';
}

type TraceLineInfo = { type: TraceContributorType };

function buildTraceLineLookup(ranges: TraceRange[]) {
  const lookup = new Map<number, TraceLineInfo>();

  for (const range of ranges) {
    const type = range.contributor?.type ?? 'unknown';
    for (let line = range.startLine; line <= range.endLine; line += 1) {
      if (!lookup.has(line)) lookup.set(line, { type });
    }
  }

  return lookup;
}

function traceClass(type: TraceContributorType) {
  if (type === 'ai') return 'diff-line-trace-ai';
  if (type === 'human') return 'diff-line-trace-human';
  if (type === 'mixed') return 'diff-line-trace-mixed';
  return 'diff-line-trace-unknown';
}

function parseNewFileLineNumber(hunkLine: string) {
  const match = /^@@\\s+-(\\d+),(\\d+)\\s+\\+(\\d+),(\\d+)\\s+@@/.exec(hunkLine);
  if (!match) return null;
  return Number(match[3]);
}

export function DiffViewer(props: {
  title: string;
  diffText: string | null;
  loading?: boolean;
  traceRanges?: TraceRange[];
}) {
  const { title, diffText, loading, traceRanges } = props;
  const traceLookup = traceRanges ? buildTraceLineLookup(traceRanges) : null;

  return (
    <div className="card flex h-full flex-col overflow-hidden">
      <div className="flex items-center justify-between gap-3 border-b border-stone-100 px-4 py-3 bg-stone-50/50">
        <div className="truncate font-mono text-[12px] text-stone-600">{title}</div>
        {loading ? <div className="text-xs text-stone-400">Loading…</div> : null}
      </div>

      <div className="flex-1 overflow-auto px-4 py-3 bg-white">
        {!diffText ? (
          <div className="text-sm text-stone-400">Select a file to view its diff.</div>
        ) : (
          <pre className="text-[12px] leading-relaxed text-stone-700 font-mono">
            {(() => {
              let currentLineNumber = 0;
              let inHunk = false;

              return diffText.split(/\r?\n/).map((line, idx) => {
                if (line.startsWith('@@')) {
                  const nextLine = parseNewFileLineNumber(line);
                  if (nextLine !== null) {
                    currentLineNumber = nextLine;
                    inHunk = true;
                  }
                  return (
                    <div key={idx} className={`${lineClass(line)} px-2 -mx-2`}>
                      {line || ' '}
                    </div>
                  );
                }

                let traceStyle = '';
                if (inHunk && traceLookup) {
                  const traceInfo = traceLookup.get(currentLineNumber);
                  if (traceInfo) traceStyle = traceClass(traceInfo.type);
                }

                const classes = [lineClass(line), traceStyle, 'px-2', '-mx-2'].filter(Boolean).join(' ');

                if (inHunk) {
                  if (line.startsWith('+') && !line.startsWith('+++')) {
                    currentLineNumber += 1;
                  } else if (!line.startsWith('-') || line.startsWith('---')) {
                    currentLineNumber += 1;
                  }
                }

                return (
                  <div key={idx} className={classes}>
                    {line || ' '}
                  </div>
                );
              });
            })()}
          </pre>
        )}
      </div>
    </div>
  );
}
