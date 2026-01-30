import clsx from 'clsx';
import { FileText, FolderOpen, GitBranch, LayoutGrid, Network } from 'lucide-react';
import { type ReactNode, useEffect, useRef, useState } from 'react';

export type Mode = 'demo' | 'repo' | 'speculate';

export function TopNav(props: {
  mode: Mode;
  onModeChange: (mode: Mode) => void;
  repoPath?: string | null;
  onOpenRepo: () => void;
  onImportSession?: () => void;
  onImportKimiSession?: () => void;
  onImportAgentTrace?: () => void;
  importEnabled?: boolean;
  children?: ReactNode;
}) {
  const {
    mode,
    onModeChange,
    repoPath,
    onOpenRepo,
    onImportSession,
    onImportKimiSession,
    onImportAgentTrace,
    importEnabled,
    children
  } = props;

  const Tab = (p: { id: Mode; label: string; icon: ReactNode }) => (
    <button
      role="tab"
      aria-selected={mode === p.id}
      tabIndex={mode === p.id ? 0 : -1}
      className={clsx(
        'inline-flex items-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition-colors',
        mode === p.id
          ? 'bg-white text-stone-800 shadow-sm'
          : 'text-stone-500 hover:bg-stone-100 hover:text-stone-700'
      )}
      onClick={() => onModeChange(p.id)}
      type="button"
    >
      {p.icon}
      <span>{p.label}</span>
    </button>
  );

  const handleTabKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const order: Mode[] = ['demo', 'repo', 'speculate'];
    const currentIndex = order.indexOf(mode);
    if (currentIndex === -1) return;
    if (event.key === 'Home') {
      onModeChange(order[0]);
      return;
    }
    if (event.key === 'End') {
      onModeChange(order[order.length - 1]);
      return;
    }
    const delta = event.key === 'ArrowRight' ? 1 : -1;
    const nextIndex = (currentIndex + delta + order.length) % order.length;
    onModeChange(order[nextIndex]);
  };

  return (
    <div className="flex items-center justify-between border-b border-stone-200 bg-white px-4 py-3">
      <div className="flex items-center gap-3">
        <div className="text-sm font-bold tracking-wide text-stone-800">Narrative</div>
        <div
          className="flex items-center gap-1 bg-stone-100 rounded-lg p-1"
          role="tablist"
          aria-label="View mode"
          onKeyDown={handleTabKeyDown}
        >
          <Tab id="demo" label="Demo" icon={<LayoutGrid className="h-4 w-4" />} />
          <Tab id="repo" label="Repo" icon={<GitBranch className="h-4 w-4" />} />
          <Tab id="speculate" label="Speculate" icon={<Network className="h-4 w-4" />} />
        </div>
      </div>

      <div className="flex items-center gap-3">
        {repoPath ? (
          <div className="max-w-[44ch] truncate text-xs text-stone-400" title={repoPath}>
            {repoPath}
          </div>
        ) : null}

        {mode === 'repo' && (
          <ImportMenu
            onImportSession={onImportSession}
            onImportKimiSession={onImportKimiSession}
            onImportAgentTrace={onImportAgentTrace}
            importEnabled={importEnabled}
          />
        )}

        {children}

        <button
          type="button"
          className="inline-flex items-center gap-2 rounded-lg bg-stone-800 px-3 py-1.5 text-sm font-medium text-white hover:bg-stone-700 transition-colors"
          onClick={onOpenRepo}
        >
          <FolderOpen className="h-4 w-4" />
          Open repo…
        </button>
      </div>
    </div>
  );
}

function ImportMenu(props: {
  onImportSession?: () => void;
  onImportKimiSession?: () => void;
  onImportAgentTrace?: () => void;
  importEnabled?: boolean;
}) {
  const { onImportSession, onImportKimiSession, onImportAgentTrace, importEnabled } = props;
  const [isOpen, setIsOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  if (!onImportSession && !onImportKimiSession && !onImportAgentTrace) return null;

  return (
    <div className="relative" ref={menuRef}>
      <button
        type="button"
        disabled={!importEnabled}
        onClick={() => setIsOpen(!isOpen)}
        className={clsx(
          'inline-flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors',
          importEnabled
            ? 'bg-stone-100 text-stone-700 hover:bg-stone-200'
            : 'bg-stone-50 text-stone-400 cursor-not-allowed'
        )}
      >
        <FileText className="h-4 w-4" />
        Import data…
      </button>

      {isOpen && (
        <div className="absolute right-0 top-full mt-2 w-56 rounded-xl border border-stone-200 bg-white p-1 shadow-lg z-50 flex flex-col gap-0.5">
          {onImportSession && (
            <button
              type="button"
              onClick={() => {
                onImportSession();
                setIsOpen(false);
              }}
              className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-stone-700 hover:bg-stone-100 text-left"
            >
              Import session JSON…
            </button>
          )}
          {onImportKimiSession && (
            <button
              type="button"
              onClick={() => {
                onImportKimiSession();
                setIsOpen(false);
              }}
              className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-stone-700 hover:bg-stone-100 text-left"
            >
              Import Kimi log…
            </button>
          )}
          {onImportAgentTrace && (
            <button
              type="button"
              onClick={() => {
                onImportAgentTrace();
                setIsOpen(false);
              }}
              className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-sm text-stone-700 hover:bg-stone-100 text-left"
            >
              Import Agent Trace…
            </button>
          )}
        </div>
      )}
    </div>
  );
}
