import { AuthorBadge, getLineColor, type SourceLine } from './AuthorBadge';

export interface SourceLensLineTableProps {
  lines: SourceLine[];
}

export function SourceLensLineTable({ lines }: SourceLensLineTableProps) {
  return (
    <div className="max-h-[400px] overflow-auto font-mono text-xs">
      {lines.map((line) => (
        <div
          key={line.lineNumber}
          className={`flex items-start gap-3 px-4 py-1.5 border-b border-stone-50 last:border-0 transition-colors ${getLineColor(line.authorType)}`}
        >
          <span className="w-10 text-right text-stone-400 select-none shrink-0">
            {line.lineNumber}
          </span>
          <div className="w-24 shrink-0 pt-0.5">
            <AuthorBadge line={line} />
          </div>
          <span className="flex-1 text-stone-700 whitespace-pre">
            {line.content || ' '}
          </span>
        </div>
      ))}
    </div>
  );
}
