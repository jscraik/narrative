import { useState } from 'react';
import { MessageSquare, Activity, Settings, TestTube, FileCode } from 'lucide-react';
import type { SessionExcerpt, TestRun, TraceCommitSummary, TraceCollectorStatus, TraceCollectorConfig, TraceRange } from '../../core/types';
import { SessionExcerpts } from './SessionExcerpts';
import { TraceTranscriptPanel } from './TraceTranscriptPanel';
import { AgentTraceSummary } from './AgentTraceSummary';
import { CodexOtelSettingsPanel } from './CodexOtelSettingsPanel';
import { TestResultsPanel } from './TestResultsPanel';
import { DiffViewer } from './DiffViewer';
import { SourceLensView } from './SourceLensView';

type TabId = 'session' | 'attribution' | 'settings' | 'tests';

interface TabConfig {
  id: TabId;
  label: string;
  icon: typeof MessageSquare;
}

const TABS: TabConfig[] = [
  { id: 'session', label: 'Session', icon: MessageSquare },
  { id: 'attribution', label: 'AI Attribution', icon: Activity },
  { id: 'settings', label: 'Settings', icon: Settings },
  { id: 'tests', label: 'Tests', icon: TestTube },
];

interface RightPanelTabsProps {
  // Session data
  sessionExcerpts?: SessionExcerpt[];
  selectedFile: string | null;
  onFileClick: (path: string) => void;
  onUnlinkSession?: (sessionId: string) => void;
  onCommitClick: (commitSha: string) => void;
  selectedCommitId: string | null;
  
  // Attribution data
  traceSummary?: TraceCommitSummary;
  traceStatus?: TraceCollectorStatus;
  hasFiles: boolean;
  onExportAgentTrace?: () => void;
  onRunOtlpSmokeTest?: () => void;
  
  // Settings data
  traceConfig?: TraceCollectorConfig;
  onUpdateCodexOtelPath?: (path: string) => void;
  onToggleCodexOtelReceiver?: (enabled: boolean) => void;
  onOpenCodexOtelDocs?: () => void;
  codexPromptExport?: { enabled: boolean | null; configPath: string | null };
  
  // Test data
  testRun?: TestRun;
  onTestFileClick: (path: string) => void;
  
  // Diff data
  selectedCommitSha: string | null;
  repoId?: number;
  diffText: string | null;
  loadingDiff: boolean;
  traceRanges: TraceRange[];
}

export function RightPanelTabs(props: RightPanelTabsProps) {
  const [activeTab, setActiveTab] = useState<TabId>('session');
  const [diffExpanded, setDiffExpanded] = useState(true);

  const {
    sessionExcerpts,
    selectedFile,
    onFileClick,
    onUnlinkSession,
    onCommitClick,
    selectedCommitId,
    traceSummary,
    traceStatus,
    hasFiles,
    onExportAgentTrace,
    onRunOtlpSmokeTest,
    traceConfig,
    onUpdateCodexOtelPath,
    onToggleCodexOtelReceiver,
    onOpenCodexOtelDocs,
    codexPromptExport,
    testRun,
    onTestFileClick,
    selectedCommitSha,
    repoId,
    diffText,
    loadingDiff,
    traceRanges,
  } = props;

  // Determine which tabs have content
  const hasSessionContent = sessionExcerpts && sessionExcerpts.length > 0;
  const hasAttributionContent = traceSummary || traceStatus;
  const hasTestContent = testRun && testRun.tests.length > 0;

  // Auto-switch to attribution tab if no session but has attribution
  // This is a one-time effect that runs when content becomes available
  const effectiveTab = (() => {
    if (activeTab === 'session' && !hasSessionContent && hasAttributionContent) {
      return 'attribution';
    }
    return activeTab;
  })();

  return (
    <div className="flex flex-col h-full gap-4">
      {/* Tab Navigation */}
      <div className="card p-2">
        <div className="flex gap-1">
          {TABS.map((tab) => {
            const Icon = tab.icon;
            const isActive = effectiveTab === tab.id;
            const hasContent = 
              (tab.id === 'session' && hasSessionContent) ||
              (tab.id === 'attribution' && hasAttributionContent) ||
              (tab.id === 'tests' && hasTestContent) ||
              tab.id === 'settings';

            return (
              <button
                key={tab.id}
                type="button"
                onClick={() => setActiveTab(tab.id)}
                className={`
                  flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-xs font-medium
                  transition-all duration-150
                  ${isActive 
                    ? 'bg-sky-100 text-sky-700' 
                    : 'text-stone-500 hover:bg-stone-100 hover:text-stone-700'
                  }
                  ${!hasContent && tab.id !== 'settings' ? 'opacity-60' : ''}
                `}
                aria-selected={isActive}
                role="tab"
              >
                <Icon className="w-3.5 h-3.5" />
                <span className="hidden sm:inline">{tab.label}</span>
              </button>
            );
          })}
        </div>
      </div>

      {/* Tab Content */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {effectiveTab === 'session' && (
          <div className="flex flex-col gap-4">
            <SessionExcerpts
              excerpts={sessionExcerpts}
              selectedFile={selectedFile}
              onFileClick={onFileClick}
              onUnlink={onUnlinkSession}
              onCommitClick={onCommitClick}
              selectedCommitId={selectedCommitId}
            />
            <TraceTranscriptPanel
              excerpt={sessionExcerpts?.[0]}
              selectedFile={selectedFile}
              onFileClick={onFileClick}
            />
          </div>
        )}

        {effectiveTab === 'attribution' && (
          <AgentTraceSummary
            summary={traceSummary}
            hasFiles={hasFiles}
            status={traceStatus}
            onExport={onExportAgentTrace}
            onSmokeTest={onRunOtlpSmokeTest}
          />
        )}

        {effectiveTab === 'settings' && (
          <CodexOtelSettingsPanel
            traceConfig={traceConfig}
            onUpdateCodexOtelPath={onUpdateCodexOtelPath}
            onToggleCodexOtelReceiver={onToggleCodexOtelReceiver}
            onOpenCodexOtelDocs={onOpenCodexOtelDocs}
            logUserPromptEnabled={codexPromptExport?.enabled ?? null}
            logUserPromptConfigPath={codexPromptExport?.configPath ?? null}
          />
        )}

        {effectiveTab === 'tests' && (
          <TestResultsPanel testRun={testRun} onFileClick={onTestFileClick} />
        )}

        {/* Source Lens - shown in all tabs when applicable */}
        {repoId && selectedCommitSha && selectedFile && (
          <div className="mt-4">
            <SourceLensView
              repoId={repoId}
              commitSha={selectedCommitSha}
              filePath={selectedFile}
            />
          </div>
        )}
      </div>

      {/* Diff Viewer - Always visible at bottom */}
      <div className="flex-none">
        <button
          type="button"
          onClick={() => setDiffExpanded(!diffExpanded)}
          className="w-full flex items-center justify-between px-4 py-2 bg-stone-100 hover:bg-stone-200 rounded-t-lg text-xs font-medium text-stone-600 transition-colors"
        >
          <span className="flex items-center gap-2">
            <FileCode className="w-3.5 h-3.5" />
            {selectedFile ? selectedFile.split('/').pop() : 'Diff'}
          </span>
          <span>{diffExpanded ? '▼' : '▲'}</span>
        </button>
        {diffExpanded && (
          <div className="card rounded-t-none border-t-0 max-h-[400px] overflow-auto">
            <DiffViewer
              title=""
              diffText={diffText}
              loading={loadingDiff}
              traceRanges={traceRanges}
            />
          </div>
        )}
      </div>
    </div>
  );
}
