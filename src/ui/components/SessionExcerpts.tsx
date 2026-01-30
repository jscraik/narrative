import { ChevronDown, ChevronUp, Link2, Link2Off, Upload } from 'lucide-react';
import { useState } from 'react';
import type { SessionExcerpt } from '../../core/types';
import { Dialog } from './Dialog';

function CollapsibleText({ text }: { text: string }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const maxLength = 500;

  if (text.length <= maxLength) {
    return (
      <div className="text-sm text-stone-700 leading-relaxed whitespace-pre-wrap break-words">
        {text}
      </div>
    );
  }

  return (
    <div>
      <div className={`text-sm text-stone-700 leading-relaxed whitespace-pre-wrap break-words relative ${!isExpanded ? 'max-h-[200px] overflow-hidden' : ''
        }`}>
        {text}
        {!isExpanded && (
          <div className="absolute inset-x-0 bottom-0 h-16 bg-gradient-to-t from-white via-white/80 to-transparent pointer-events-none" />
        )}
      </div>
      <button
        type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        aria-label={isExpanded ? "Show less" : "Show more"}
        className="mt-2 text-xs font-medium text-stone-400 hover:text-stone-600 flex items-center gap-1.5 transition-colors select-none"
      >
        {isExpanded ? (
          <>Show less <ChevronUp className="w-3 h-3" /></>
        ) : (
          <>Show more <ChevronDown className="w-3 h-3" /></>
        )}
      </button>
    </div>
  );
}

function ToolPill({ tool, durationMin }: { tool: string; durationMin?: number }) {
  return (
    <div className="flex items-center gap-2 text-[11px] text-stone-400">
      <span className="px-2 py-1 bg-stone-100 rounded-md font-mono text-stone-500">
        {tool}
      </span>
      {typeof durationMin === 'number' && (
        <span>{durationMin} min</span>
      )}
    </div>
  );
}

function LinkStatus({ excerpt, onUnlink, onClick, isSelected }: {
  excerpt: SessionExcerpt;
  onUnlink?: () => void;
  onClick?: () => void;
  isSelected?: boolean;
}) {
  if (!excerpt.linkedCommitSha) {
    return (
      <div className="flex items-center gap-2 text-[11px] text-stone-500">
        <Link2Off className="w-3 h-3" />
        <span>Not linked</span>
      </div>
    );
  }

  const shortSha = excerpt.linkedCommitSha.slice(0, 8);
  const confidencePercent = excerpt.linkConfidence ? Math.round(excerpt.linkConfidence * 100) : 0;
  const isAutoLinked = excerpt.autoLinked ?? false;

  return (
    <div className="flex items-center gap-2 text-[11px] text-stone-400">
      <Link2 className="w-3 h-3" />
      <button
        type="button"
        onClick={onClick}
        aria-label={`View commit ${shortSha} in timeline`}
        className={`
          text-stone-600 hover:text-sky-600 transition-colors
          ${isSelected ? 'text-sky-600 font-semibold' : ''}
        `}
        title="Click to view this commit in the timeline"
      >
        Linked to <span className="font-mono">{shortSha}</span>
      </button>
      <span className="px-1.5 py-0.5 bg-stone-100 rounded text-stone-500">
        {confidencePercent}%
      </span>
      {isAutoLinked && (
        <span className="px-1.5 py-0.5 bg-emerald-50 rounded text-emerald-600">
          Auto
        </span>
      )}
      {onUnlink && (
        <button
          type="button"
          onClick={onUnlink}
          aria-label="Unlink session from commit"
          className="px-1.5 py-0.5 bg-red-50 hover:bg-red-100 rounded text-red-600 transition-colors"
          title="Unlink this session from the commit"
        >
          Unlink
        </button>
      )}
    </div>
  );
}

function FilePill({
  file,
  onClick,
  isSelected
}: {
  file: string;
  onClick?: () => void;
  isSelected?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={isSelected ? `View file ${file} (selected)` : `View file ${file}`}
      aria-pressed={isSelected}
      title={file}
      className={`pill-file max-w-full truncate ${isSelected ? 'selected' : ''}`}
    >
      {file}
    </button>
  );
}

function UnlinkConfirmDialog({
  isOpen,
  onClose,
  onConfirm
}: {
  isOpen: boolean;
  onClose: () => void;
  onConfirm: () => void;
}) {
  return (
    <Dialog
      title="Unlink session from commit?"
      message="This will remove the association between the AI session and the commit. The session will remain imported but will show as 'Not linked'."
      confirmLabel="Unlink"
      cancelLabel="Cancel"
      variant="destructive"
      open={isOpen}
      onConfirm={onConfirm}
      onCancel={onClose}
    />
  );
}

export function SessionExcerpts({
  excerpts,
  selectedFile,
  onFileClick,
  onUnlink,
  onCommitClick,
  selectedCommitId
}: {
  excerpts: SessionExcerpt[] | undefined;
  selectedFile?: string | null;
  onFileClick?: (path: string) => void;
  onUnlink?: (sessionId: string) => void;
  onCommitClick?: (commitSha: string) => void;
  selectedCommitId?: string | null;
}) {
  const [unlinkDialogOpen, setUnlinkDialogOpen] = useState(false);
  const [pendingUnlinkId, setPendingUnlinkId] = useState<string | null>(null);

  if (!excerpts || excerpts.length === 0) {
    return (
      <div className="card p-5 overflow-x-hidden">
        <div className="flex items-center justify-between">
          <div>
            <div className="section-header">SESSION SUMMARY</div>
            <div className="section-subheader mt-0.5">Key moments from the session</div>
          </div>
        </div>
        <div className="mt-6 flex flex-col items-center text-center py-4">
          <div className="w-12 h-12 rounded-full bg-stone-100 flex items-center justify-center mb-3">
            <Upload className="w-5 h-5 text-stone-400" />
          </div>
          <p className="text-sm text-stone-500 mb-1">No sessions imported yet</p>
          <p className="text-xs text-stone-400 mb-4">Import from Claude, Cursor, or Kimi</p>
        </div>
      </div>
    );
  }

  const excerpt = excerpts[0];

  const linkedCommitSha = excerpt.linkedCommitSha ?? null;

  const handleUnlinkClick = () => {
    setPendingUnlinkId(excerpt.id);
    setUnlinkDialogOpen(true);
  };

  const handleUnlinkConfirm = () => {
    if (pendingUnlinkId && onUnlink) {
      onUnlink(pendingUnlinkId);
    }
    setUnlinkDialogOpen(false);
    setPendingUnlinkId(null);
  };

  const handleUnlinkCancel = () => {
    setUnlinkDialogOpen(false);
    setPendingUnlinkId(null);
  };

  return (
    <>
      <div className="card p-5 overflow-x-hidden">
        <div className="flex items-center justify-between">
          <div>
            <div className="section-header">SESSION SUMMARY</div>
            <div className="section-subheader mt-0.5">Key moments from the session</div>
          </div>
          <div className="flex flex-col items-end gap-1">
            <ToolPill tool={excerpt.tool} durationMin={excerpt.durationMin} />
            <LinkStatus
              excerpt={excerpt}
              onUnlink={onUnlink && linkedCommitSha ? handleUnlinkClick : undefined}
              onClick={linkedCommitSha && onCommitClick ? () => onCommitClick(linkedCommitSha) : undefined}
              isSelected={selectedCommitId === linkedCommitSha}
            />
          </div>
        </div>

        <div className="mt-4 space-y-4">
          {excerpt.messages.map((m) => (
            <div
              key={m.id}
              className={m.role === 'user' ? 'message-user p-3' : 'message-assistant p-3'}
            >
              <div className={`text-[10px] font-bold tracking-wider uppercase mb-1 ${m.role === 'user' ? 'text-sky-600' : 'text-emerald-600'
                }`}>
                {m.role}
              </div>
              <CollapsibleText text={m.text} />
              {m.files && m.files.length > 0 ? (
                <div className="mt-3 flex flex-wrap gap-2">
                  {m.files.map((f) => (
                    <FilePill
                      key={f}
                      file={f}
                      isSelected={selectedFile === f}
                      onClick={() => onFileClick?.(f)}
                    />
                  ))}
                </div>
              ) : null}
            </div>
          ))}
        </div>
      </div>

      <UnlinkConfirmDialog
        isOpen={unlinkDialogOpen}
        onClose={handleUnlinkCancel}
        onConfirm={handleUnlinkConfirm}
      />
    </>
  );
}
