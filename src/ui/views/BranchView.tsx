import { useEffect, useMemo, useRef, useState } from 'react';
import { FileSelectionProvider, useFileSelection } from '../../core/context/FileSelectionContext';
import { testRuns } from '../../core/demo/nearbyGridDemo';
import type { BranchViewModel, FileChange, TestRun, TraceRange } from '../../core/types';
import { AgentTraceSummary } from '../components/AgentTraceSummary';
import { BranchHeader } from '../components/BranchHeader';
import { CodexOtelSettingsPanel } from '../components/CodexOtelSettingsPanel';
import { DiffViewer } from '../components/DiffViewer';
import { FilesChanged } from '../components/FilesChanged';
import { IntentList } from '../components/IntentList';
import { SessionExcerpts } from '../components/SessionExcerpts';
import { SourceLensView } from '../components/SourceLensView';
import { TestResultsPanel } from '../components/TestResultsPanel';
import { TraceTranscriptPanel } from '../components/TraceTranscriptPanel';
import { Timeline } from '../components/Timeline';

function BranchViewInner(props: {
  model: BranchViewModel;
  loadFilesForNode: (nodeId: string) => Promise<FileChange[]>;
  loadDiffForFile: (nodeId: string, filePath: string) => Promise<string>;
  loadTraceRangesForFile: (nodeId: string, filePath: string) => Promise<TraceRange[]>;
  onExportAgentTrace: (nodeId: string, files: FileChange[]) => void;
  onRunOtlpSmokeTest: (nodeId: string, files: FileChange[]) => void;
  onUpdateCodexOtelPath?: (path: string) => void;
  onToggleCodexOtelReceiver?: (enabled: boolean) => void;
  onOpenCodexOtelDocs?: () => void;
  codexPromptExport?: { enabled: boolean | null; configPath: string | null };
  onUnlinkSession?: (sessionId: string) => void;
  actionError?: string | null;
  onDismissActionError?: () => void;
}) {
  const {
    model,
    loadFilesForNode,
    loadDiffForFile,
    loadTraceRangesForFile,
    onExportAgentTrace,
    onRunOtlpSmokeTest,
    onUpdateCodexOtelPath,
    onToggleCodexOtelReceiver,
    onOpenCodexOtelDocs,
    codexPromptExport,
    onUnlinkSession,
    actionError,
    onDismissActionError
  } = props;
  const { selectedFile, selectFile } = useFileSelection();

  const defaultSelectedId = useMemo(() => {
    const head = model.meta?.headSha;
    if (head && model.timeline.some((n) => n.id === head)) return head;
    return model.timeline[model.timeline.length - 1]?.id ?? null;
  }, [model]);

  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(defaultSelectedId);
  // Track which commits have already pulsed (once per app session)
  const pulsedCommits = useRef<Set<string>>(new Set());
  const [pulseCommitId, setPulseCommitId] = useState<string | null>(null);
  const [files, setFiles] = useState<FileChange[]>([]);
  const [diffText, setDiffText] = useState<string | null>(null);
  const [loadingFiles, setLoadingFiles] = useState(false);
  const [loadingDiff, setLoadingDiff] = useState(false);
  const [_error, setError] = useState<string | null>(null);
  const [traceRanges, setTraceRanges] = useState<TraceRange[]>([]);
  const [loadingTrace, setLoadingTrace] = useState(false);

  const selectedNode = useMemo(
    () => model.timeline.find((node) => node.id === selectedNodeId) ?? null,
    [model.timeline, selectedNodeId]
  );

  const selectedCommitSha = useMemo(() => {
    if (!selectedNode || selectedNode.type !== 'commit') return null;
    return selectedNode.id;
  }, [selectedNode]);

  // Get test run for current node
  const testRun = useMemo((): TestRun | undefined => {
    if (model.source !== 'demo') return undefined;
    const node = model.timeline.find((n) => n.id === selectedNodeId);
    if (node?.badges?.some((b) => b.type === 'test')) {
      if (selectedNodeId === 't1') return testRuns.tr1;
      if (selectedNodeId === 't2') return testRuns.tr2;
    }
    return undefined;
  }, [model, selectedNodeId]);

  // Reset selection if model changes
  useEffect(() => {
    setSelectedNodeId(defaultSelectedId);
    setFiles([]);
    selectFile(null);
    setDiffText(null);
    setError(null);
  }, [defaultSelectedId, selectFile]);

  useEffect(() => {
    if (!selectedNodeId) return;
    let cancelled = false;

    setLoadingFiles(true);
    setError(null);

    loadFilesForNode(selectedNodeId)
      .then((f) => {
        if (cancelled) return;
        setFiles(f);
        if (!selectedFile && f[0]?.path) {
          selectFile(f[0].path);
        }
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
        setFiles([]);
        selectFile(null);
      })
      .finally(() => {
        if (cancelled) return;
        setLoadingFiles(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedNodeId, loadFilesForNode, selectFile, selectedFile]);

  useEffect(() => {
    if (!selectedNodeId || !selectedFile) return;
    let cancelled = false;

    setLoadingDiff(true);
    setError(null);

    loadDiffForFile(selectedNodeId, selectedFile)
      .then((d) => {
        if (cancelled) return;
        setDiffText(d || '(no diff)');
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
        setDiffText(null);
      })
      .finally(() => {
        if (cancelled) return;
        setLoadingDiff(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedNodeId, selectedFile, loadDiffForFile]);

  useEffect(() => {
    if (!selectedNodeId || !selectedFile) return;
    let cancelled = false;

    setLoadingTrace(true);
    loadTraceRangesForFile(selectedNodeId, selectedFile)
      .then((ranges) => {
        if (cancelled) return;
        setTraceRanges(ranges);
      })
      .catch(() => {
        if (cancelled) return;
        setTraceRanges([]);
      })
      .finally(() => {
        if (cancelled) return;
        setLoadingTrace(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedNodeId, selectedFile, loadTraceRangesForFile]);

  // Pulse commit badge once on first successful import
  useEffect(() => {
    // Find commits with session badges (linked sessions)
    const linkedCommits = model.timeline.filter(node =>
      node.badges?.some(b => b.type === 'session')
    );

    for (const commit of linkedCommits) {
      if (!pulsedCommits.current.has(commit.id)) {
        // First time seeing this linked commit - trigger pulse once
        pulsedCommits.current.add(commit.id);
        setPulseCommitId(commit.id);

        // Clear pulse state after animation completes (1.5s animation + buffer)
        const timer = setTimeout(() => {
          setPulseCommitId(null);
        }, 1600);
        return () => clearTimeout(timer);
      }
    }
  }, [model.timeline]);

  const handleFileClickFromSession = (path: string) => {
    const fileExists = files.some((f) => f.path === path);
    if (fileExists) {
      selectFile(path);
    }
  };

  const handleFileClickFromTest = (path: string) => {
    handleFileClickFromSession(path);
  };

  const handleCommitClickFromSession = (commitSha: string) => {
    setSelectedNodeId(commitSha);
  };

  return (
    <div className="flex h-full flex-col bg-[#f5f5f4]">
      <div className="flex-1 overflow-hidden">
        <div className="flex flex-col gap-5 p-5 h-full overflow-y-auto lg:grid lg:grid-cols-12 lg:overflow-hidden">
          {/* Left column */}
          <div className="flex flex-col gap-5 lg:col-span-7 lg:overflow-y-auto lg:pr-1">
            <BranchHeader model={model} />
            <IntentList items={model.intent} />
            <div>
              {loadingFiles ? (
                <div className="card p-5">
                  <div className="section-header">FILES CHANGED</div>
                  <div className="mt-4 space-y-2">
                    {['s1', 's2', 's3', 's4', 's5'].map((key) => (
                      <div key={key} className="flex items-center justify-between py-2">
                        <div className="h-4 bg-stone-200 rounded animate-pulse w-3/4" />
                        <div className="flex gap-2">
                          <div className="h-4 w-12 bg-stone-200 rounded animate-pulse" />
                          <div className="h-4 w-12 bg-stone-200 rounded animate-pulse" />
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              ) : (
                <FilesChanged
                  files={files}
                  title="FILES CHANGED"
                  traceByFile={selectedNodeId ? model.traceSummaries?.byFileByCommit[selectedNodeId] : undefined}
                />
              )}
            </div>

            {actionError ? (
              <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 flex items-start gap-3">
                <span className="text-red-500 mt-0.5">●</span>
                <div className="flex-1">{actionError}</div>
                {onDismissActionError ? (
                  <button
                    type="button"
                    onClick={onDismissActionError}
                    className="text-red-400 hover:text-red-600 transition-colors"
                    aria-label="Dismiss error"
                  >
                    ✕
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>

          {/* Right column */}
          <div className="flex flex-col gap-5 min-w-0 lg:col-span-5 lg:overflow-y-auto">
            <SessionExcerpts
              excerpts={model.sessionExcerpts}
              selectedFile={selectedFile}
              onFileClick={handleFileClickFromSession}
              onUnlink={onUnlinkSession}
              onCommitClick={handleCommitClickFromSession}
              selectedCommitId={selectedNodeId}
            />
            <TraceTranscriptPanel
              excerpt={model.sessionExcerpts?.[0]}
              selectedFile={selectedFile}
              onFileClick={handleFileClickFromSession}
            />
            <AgentTraceSummary
              summary={selectedNodeId ? model.traceSummaries?.byCommit[selectedNodeId] : undefined}
              hasFiles={files.length > 0}
              status={model.traceStatus}
              onExport={() => {
                if (!selectedNodeId) return;
                onExportAgentTrace(selectedNodeId, files);
              }}
              onSmokeTest={() => {
                if (!selectedNodeId) return;
                onRunOtlpSmokeTest(selectedNodeId, files);
              }}
            />
            <CodexOtelSettingsPanel
              traceConfig={model.traceConfig}
              onUpdateCodexOtelPath={onUpdateCodexOtelPath}
              onToggleCodexOtelReceiver={onToggleCodexOtelReceiver}
              onOpenCodexOtelDocs={onOpenCodexOtelDocs}
              logUserPromptEnabled={codexPromptExport?.enabled ?? null}
              logUserPromptConfigPath={codexPromptExport?.configPath ?? null}
            />
            {model.source === 'git' && selectedCommitSha && selectedFile && model.meta?.repoId ? (
              <SourceLensView
                repoId={model.meta.repoId}
                commitSha={selectedCommitSha}
                filePath={selectedFile}
              />
            ) : null}
            <TestResultsPanel testRun={testRun} onFileClick={handleFileClickFromTest} />
            <div className="min-h-[200px] flex-1">
              <DiffViewer
                title={selectedFile ?? 'DIFF'}
                diffText={diffText}
                loading={loadingDiff || loadingTrace}
                traceRanges={traceRanges}
              />
            </div>
          </div>
        </div>
      </div>

      <Timeline
        nodes={model.timeline}
        selectedId={selectedNodeId}
        onSelect={setSelectedNodeId}
        pulseCommitId={pulseCommitId}
      />
    </div>
  );
}

export function BranchView(props: {
  model: BranchViewModel;
  loadFilesForNode: (nodeId: string) => Promise<FileChange[]>;
  loadDiffForFile: (nodeId: string, filePath: string) => Promise<string>;
  loadTraceRangesForFile: (nodeId: string, filePath: string) => Promise<TraceRange[]>;
  onExportAgentTrace: (nodeId: string, files: FileChange[]) => void;
  onRunOtlpSmokeTest: (nodeId: string, files: FileChange[]) => void;
  onUpdateCodexOtelPath?: (path: string) => void;
  onToggleCodexOtelReceiver?: (enabled: boolean) => void;
  onOpenCodexOtelDocs?: () => void;
  codexPromptExport?: { enabled: boolean | null; configPath: string | null };
  onUnlinkSession?: (sessionId: string) => void;
  actionError?: string | null;
  onDismissActionError?: () => void;
}) {
  return (
    <FileSelectionProvider>
      <BranchViewInner {...props} />
    </FileSelectionProvider>
  );
}
