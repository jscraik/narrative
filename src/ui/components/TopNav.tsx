import type { ReactNode } from 'react';
import { FileText, FolderOpen, GitBranch, LayoutGrid, Network } from 'lucide-react';
import clsx from 'clsx';

export type Mode = 'demo' | 'repo' | 'speculate';

export function TopNav(props: {
  mode: Mode;
  onModeChange: (mode: Mode) => void;
  repoPath?: string | null;
  onOpenRepo: () => void;
  onImportSession?: () => void;
  onImportAgentTrace?: () => void;
  importEnabled?: boolean;
}) {
  const { mode, onModeChange, repoPath, onOpenRepo, onImportSession, onImportAgentTrace, importEnabled } = props;

  const Tab = (p: { id: Mode; label: string; icon: ReactNode }) => (
    <button
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

  return (
    <div className="flex items-center justify-between border-b border-stone-200 bg-white px-4 py-3">
      <div className="flex items-center gap-3">
        <div className="text-sm font-bold tracking-wide text-stone-800">Narrative</div>
        <div className="flex items-center gap-1 bg-stone-100 rounded-lg p-1">
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

        {mode === 'repo' && onImportSession ? (
          <button
            type="button"
            disabled={!importEnabled}
            className={clsx(
              'inline-flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors',
              importEnabled
                ? 'bg-stone-100 text-stone-700 hover:bg-stone-200'
                : 'bg-stone-50 text-stone-400 cursor-not-allowed'
            )}
            onClick={onImportSession}
          >
            <FileText className="h-4 w-4" />
            Import session…
          </button>
        ) : null}

        {mode === 'repo' && onImportAgentTrace ? (
          <button
            type="button"
            disabled={!importEnabled}
            className={clsx(
              'inline-flex items-center gap-2 rounded-lg px-3 py-1.5 text-sm font-medium transition-colors',
              importEnabled
                ? 'bg-stone-100 text-stone-700 hover:bg-stone-200'
                : 'bg-stone-50 text-stone-400 cursor-not-allowed'
            )}
            onClick={onImportAgentTrace}
          >
            <FileText className="h-4 w-4" />
            Import Agent Trace…
          </button>
        ) : null}

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
