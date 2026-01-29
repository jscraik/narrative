import { useEffect, useMemo, useState } from 'react';
import type { BranchViewModel, FileChange, TestRun, TraceRange } from '../../core/types';
import { FileSelectionProvider, useFileSelection } from '../../core/context/FileSelectionContext';
import { AgentTraceSummary } from '../components/AgentTraceSummary';
import { BranchHeader } from '../components/BranchHeader';
import { DiffViewer } from '../components/DiffViewer';
import { FilesChanged } from '../components/FilesChanged';
import { IntentList } from '../components/IntentList';
import { SessionExcerpts } from '../components/SessionExcerpts';
import { TestResultsPanel } from '../components/TestResultsPanel';
import { Timeline } from '../components/Timeline';
import { testRuns } from '../../core/demo/nearbyGridDemo';

function BranchViewInner(props: {
  model: BranchViewModel;
  loadFilesForNode: (nodeId: string) => Promise<FileChange[]>;
  loadDiffForFile: (nodeId: string, filePath: string) => Promise<string>;
  loadTraceRangesForFile: (nodeId: string, filePath: string) => Promise<TraceRange[]>;
  onExportAgentTrace: (nodeId: string, files: FileChange[]) => void;
}) {
  const { model, loadFilesForNode, loadDiffForFile, loadTraceRangesForFile, onExportAgentTrace } = props;
  const { selectedFile, selectFile } = useFileSelection();

  const defaultSelectedId = useMemo(() => {
    const head = model.meta?.headSha;
    if (head && model.timeline.some((n) => n.id === head)) return head;
    return model.timeline[model.timeline.length - 1]?.id ?? null;
  }, [model]);

  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(defaultSelectedId);
  const [files, setFiles] = useState<FileChange[]>([]);
  const [diffText, setDiffText] = useState<string | null>(null);
  const [loadingFiles, setLoadingFiles] = useState(false);
  const [loadingDiff, setLoadingDiff] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [traceRanges, setTraceRanges] = useState<TraceRange[]>([]);
  const [loadingTrace, setLoadingTrace] = useState(false);

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

  const handleFileClickFromSession = (path: string) => {
    const fileExists = files.some((f) => f.path === path);
    if (fileExists) {
      selectFile(path);
    }
  };

  const handleFileClickFromTest = (path: string) => {
    handleFileClickFromSession(path);
  };

  return (
    <div className="flex h-full flex-col bg-[#f5f5f4]">
      <div className="flex-1 overflow-hidden">
        <div className="grid h-full grid-cols-12 gap-5 p-5">
          {/* Left column */}
          <div className="col-span-7 flex flex-col gap-5 overflow-y-auto pr-1">
            <BranchHeader model={model} />
            <IntentList items={model.intent} />
            <div>
              {loadingFiles ? (
                <div className="card p-5 text-sm text-stone-400">
                  Loading files…
                </div>
              ) : (
                <FilesChanged files={files} title="FILES CHANGED" traceByFile={model.traceSummaries?.byFile} />
              )}
            </div>

            {error ? (
              <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700">
                {error}
              </div>
            ) : null}
          </div>

          {/* Right column */}
          <div className="col-span-5 flex flex-col gap-5 overflow-y-auto">
            <SessionExcerpts
              excerpts={model.sessionExcerpts}
              selectedFile={selectedFile}
              onFileClick={handleFileClickFromSession}
            />
            <AgentTraceSummary
              summary={selectedNodeId ? model.traceSummaries?.byCommit[selectedNodeId] : undefined}
              hasFiles={files.length > 0}
              onExport={() => {
                if (!selectedNodeId) return;
                onExportAgentTrace(selectedNodeId, files);
              }}
            />
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

      <Timeline nodes={model.timeline} selectedId={selectedNodeId} onSelect={setSelectedNodeId} />
    </div>
  );
}

export function BranchView(props: {
  model: BranchViewModel;
  loadFilesForNode: (nodeId: string) => Promise<FileChange[]>;
  loadDiffForFile: (nodeId: string, filePath: string) => Promise<string>;
  loadTraceRangesForFile: (nodeId: string, filePath: string) => Promise<TraceRange[]>;
  onExportAgentTrace: (nodeId: string, files: FileChange[]) => void;
}) {
  return (
    <FileSelectionProvider>
      <BranchViewInner {...props} />
    </FileSelectionProvider>
  );
}
