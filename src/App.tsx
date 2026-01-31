import { useCallback, useEffect, useState } from 'react';
import { setOtelReceiverEnabled } from './core/tauri/otelReceiver';
import type {
  BranchViewModel,
  TraceCollectorConfig
} from './core/types';
import { RepoEmptyState } from './ui/components/RepoEmptyState';
import { TopNav, type Mode } from './ui/components/TopNav';
import { BranchView } from './ui/views/BranchView';
import { SpeculateView } from './ui/views/SpeculateView';
import { DocsOverviewPanel } from './ui/components/DocsOverviewPanel';
import { useRepoLoader, type RepoState } from './hooks/useRepoLoader';
import { useUpdater } from './hooks/useUpdater';
import { useTraceCollector } from './hooks/useTraceCollector';
import { useSessionImport } from './hooks/useSessionImport';
import { useCommitData } from './hooks/useCommitData';
import { UpdatePrompt, UpdateIndicator } from './ui/components/UpdatePrompt';
import { indexRepo } from './core/repo/indexer';

/**
 * Docs view wrapper that auto-loads the current directory as repo if needed.
 * This ensures Docs mode works even when switching from Demo mode.
 */
function DocsView(props: {
  repoState: RepoState;
  setRepoState: React.Dispatch<React.SetStateAction<RepoState>>;
  onClose: () => void;
}) {
  const { repoState, setRepoState, onClose } = props;
  const [isLoading, setIsLoading] = useState(false);

  // Auto-load current directory as repo when Docs is opened without a loaded repo
  useEffect(() => {
    if (repoState.status !== 'idle' && repoState.status !== 'error') {
      return; // Already loaded or loading
    }

    const loadCurrentDir = async () => {
      if (!import.meta.env.DEV) {
        return;
      }

      setIsLoading(true);
      try {
        // Dev-only fallback to a local repo path
        const defaultPath = '/Users/jamiecraik/dev/narrative';

        setRepoState({ status: 'loading', path: defaultPath });

        const { model, repo } = await indexRepo(defaultPath, 60);
        setRepoState({ status: 'ready', path: defaultPath, model, repo });
      } catch (e) {
        console.error('[DocsView] Failed to auto-load repo:', e);
        // Don't change state on error - let the UI show "No Repository Open"
        setRepoState({ status: 'idle' });
      } finally {
        setIsLoading(false);
      }
    };

    loadCurrentDir();
  }, [repoState.status, setRepoState]);

  if (repoState.status === 'loading' || isLoading) {
    return (
      <div className="h-full p-4 flex items-center justify-center">
        <div className="text-center text-stone-500">
          <div className="text-sm">Loading repository...</div>
        </div>
      </div>
    );
  }

  return (
    <div className="h-full p-4 overflow-hidden">
      <DocsOverviewPanel 
        repoRoot={repoState.status === 'ready' ? repoState.repo.root : ''}
        onClose={onClose}
      />
    </div>
  );
}

export default function App() {
  const [mode, setMode] = useState<Mode>('demo');

  // Repo loading and indexing
  const {
    repoState,
    setRepoState,
    indexingProgress,
    codexPromptExport,
    attributionPrefs,
    actionError,
    setActionError,
    openRepo,
    updateAttributionPrefs,
    purgeAttributionMetadata,
    diffCache
  } = useRepoLoader();

  // OTLP trace collection events and handlers
  const traceCollectorHandlers = useTraceCollector({
    repoRoot: repoState.status === 'ready' ? repoState.repo.root : '',
    repoId: repoState.status === 'ready' ? repoState.repo.repoId : 0,
    timeline: repoState.status === 'ready' ? repoState.model.timeline : [],
    setRepoState: (updater) => {
      setRepoState((prev) => {
        if (prev.status !== 'ready') return prev;
        return { ...prev, model: updater(prev.model) };
      });
    },
    setActionError
  });

  // Session import handlers
  const sessionImportHandlers = useSessionImport({
    repoRoot: repoState.status === 'ready' ? repoState.repo.root : '',
    repoId: repoState.status === 'ready' ? repoState.repo.repoId : 0,
    model: repoState.status === 'ready' ? repoState.model : ({} as BranchViewModel),
    setRepoState: (updater) => {
      setRepoState((prev) => {
        if (prev.status !== 'ready') return prev;
        return { ...prev, model: updater(prev.model) };
      });
    },
    setActionError
  });

  // Commit data loading (model, path, files, diffs, traces)
  const commitData = useCommitData({
    mode,
    repoState,
    diffCache: diffCache as unknown as React.MutableRefObject<{ get(key: string): string | undefined; set(key: string, value: string): void }>,
    model: null // Will be computed inside the hook
  });

  // Auto-updater integration
  const { status: updateStatus, checkForUpdates, downloadAndInstall, dismiss } = useUpdater({
    checkOnMount: true, // Check for updates on app launch
    pollIntervalMinutes: 60 * 24 // Check once per day
  });

  const updateCodexOtelReceiverEnabled = useCallback(
    async (enabled: boolean) => {
      try {
        await setOtelReceiverEnabled(enabled);
        setRepoState((prev) => {
          if (prev.status !== 'ready') return prev;
          return {
            ...prev,
            model: {
              ...prev.model,
              traceConfig: {
                ...(prev.model.traceConfig ?? {} as TraceCollectorConfig),
                codexOtelReceiverEnabled: enabled
              } as TraceCollectorConfig
            }
          };
        });
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [setRepoState, setActionError]
  );

  const importEnabled = mode === 'repo' && repoState.status === 'ready';

  return (
    <div className="flex h-full flex-col bg-[#f5f5f4] text-stone-800">
      {/* Update Notification */}
      {updateStatus && (
        <UpdatePrompt
          status={updateStatus}
          onUpdate={downloadAndInstall}
          onDismiss={dismiss}
          onCheckAgain={checkForUpdates}
        />
      )}

      <TopNav
        mode={mode}
        onModeChange={setMode}
        repoPath={commitData.repoPath}
        onOpenRepo={openRepo}
        onImportSession={sessionImportHandlers.importSession}
        onImportKimiSession={sessionImportHandlers.importKimiSession}
        onImportAgentTrace={sessionImportHandlers.importAgentTrace}
        importEnabled={importEnabled}
      >
        {/* Update indicator in nav */}
        <UpdateIndicator status={updateStatus} onClick={checkForUpdates} />
      </TopNav>

      <div className="flex-1 overflow-hidden">
        {mode === 'docs' ? (
          <DocsView 
            repoState={repoState}
            setRepoState={setRepoState}
            onClose={() => setMode('repo')}
          />
        ) : mode === 'speculate' ? (
          <SpeculateView />
        ) : mode === 'repo' && repoState.status === 'loading' ? (
          <div className="p-8 text-sm text-stone-500">
            <div className="text-sm font-medium text-stone-700">Indexing repo…</div>
            <div className="mt-2 text-xs text-stone-500">
              {indexingProgress?.message ?? 'Preparing index…'}
            </div>
            <div className="mt-3 h-2 w-64 max-w-full rounded-full bg-stone-200 overflow-hidden">
              <div
                className="h-full bg-sky-500 transition-[width] duration-300"
                style={{ width: `${indexingProgress?.percent ?? 0}%` }}
              />
            </div>
            <div className="mt-2 text-xs text-stone-400">
              {indexingProgress?.total
                ? `${indexingProgress.current ?? 0}/${indexingProgress.total} · ${indexingProgress.phase}`
                : indexingProgress?.phase ?? 'loading'}
            </div>
          </div>
        ) : mode === 'repo' && repoState.status === 'error' ? (
          <div className="p-8">
            <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700">
              {repoState.message}
            </div>
            <div className="mt-4 text-sm text-stone-500">
              Ensure the selected folder is a git repository and that <span className="font-mono">git</span> is
              available on your PATH.
            </div>
          </div>
        ) : commitData.model ? (
          <BranchView
            model={commitData.model}
            loadFilesForNode={commitData.loadFilesForNode}
            loadDiffForFile={commitData.loadDiffForFile}
            loadTraceRangesForFile={commitData.loadTraceRangesForFile}
            onExportAgentTrace={traceCollectorHandlers.exportAgentTrace}
            onRunOtlpSmokeTest={traceCollectorHandlers.runOtlpSmokeTestHandler}
            onUpdateCodexOtelPath={traceCollectorHandlers.updateCodexOtelPath}
            onToggleCodexOtelReceiver={updateCodexOtelReceiverEnabled}
            onOpenCodexOtelDocs={traceCollectorHandlers.openCodexOtelDocs}
            codexPromptExport={codexPromptExport}
            attributionPrefs={attributionPrefs}
            onUpdateAttributionPrefs={updateAttributionPrefs}
            onPurgeAttributionMetadata={purgeAttributionMetadata}
            onUnlinkSession={sessionImportHandlers.unlinkSession}
            actionError={actionError}
            onDismissActionError={() => setActionError(null)}
          />
        ) : (
          <RepoEmptyState onOpenRepo={openRepo} />
        )}
      </div>
    </div>
  );
}
