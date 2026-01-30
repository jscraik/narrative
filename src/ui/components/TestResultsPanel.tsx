import { useState } from 'react';
import { ChevronDown, ChevronUp, XCircle, CheckCircle, HelpCircle, Clock, Terminal } from 'lucide-react';
import type { TestRun, TestCase } from '../../core/types';

function TestCaseRow({ test, onFileClick }: { test: TestCase; onFileClick?: (path: string) => void }) {
  const filePath = test.filePath;
  const handleFileClick = () => {
    if (!filePath) return;
    onFileClick?.(filePath);
  };

  return (
    <div className="border-b border-red-100 py-3 last:border-0">
      <div className="flex items-start gap-3">
        <XCircle className="mt-0.5 h-4 w-4 shrink-0 text-red-500" />
        <div className="flex-1 min-w-0">
          <div className="text-sm text-stone-700">{test.name}</div>
          {test.errorMessage && (
            <div className="mt-1 text-xs text-red-600 font-mono">{test.errorMessage}</div>
          )}
          {filePath ? (
            <button
              type="button"
              onClick={handleFileClick}
              className="mt-2 pill-file"
            >
              {filePath}
            </button>
          ) : null}
        </div>
        <div className="text-[11px] text-stone-400 tabular-nums">
          {(test.durationMs / 1000).toFixed(2)}s
        </div>
      </div>
    </div>
  );
}

export function TestResultsPanel({
  testRun,
  onFileClick,
  className
}: {
  testRun?: TestRun;
  onFileClick?: (path: string) => void;
  className?: string;
}) {
  const [expanded, setExpanded] = useState(true);
  const panelId = "test-results-panel";

  if (!testRun) {
    return (
      <div className={`card p-5 ${className || ''}`}>
        <div className="section-header">TEST RESULTS</div>
        <div className="mt-5 flex flex-col items-center text-center py-3">
          <div className="w-10 h-10 rounded-full bg-stone-100 flex items-center justify-center mb-2">
            <Terminal className="w-4 h-4 text-stone-400" />
          </div>
          <p className="text-sm text-stone-500 mb-1">No test data available</p>
          <p className="text-xs text-stone-400">Import from CI or run tests locally</p>
        </div>
      </div>
    );
  }

  const failedTests = testRun.tests.filter((t) => t.status === 'failed');
  const hasFailures = failedTests.length > 0;

  return (
    <div className={`card overflow-hidden ${className || ''}`}>
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center justify-between gap-3 p-5 hover:bg-stone-50 transition-colors"
        aria-expanded={expanded}
        aria-controls={panelId}
      >
        <div className="flex-1">
          <div className="flex items-center gap-3">
            <div className="section-header">TEST RESULTS</div>
            {hasFailures && (
              <span className="pill-test-failed">
                <XCircle className="h-3 w-3" />
                {failedTests.length} failed
              </span>
            )}
          </div>
          <div className="mt-2 flex items-center gap-4 text-[11px] text-stone-500">
            <span className="flex items-center gap-1.5">
              <XCircle className="h-3.5 w-3.5 text-red-500" />
              {testRun.failed} failed
            </span>
            <span className="flex items-center gap-1.5">
              <CheckCircle className="h-3.5 w-3.5 text-emerald-500" />
              {testRun.passed} passed
            </span>
            <span className="flex items-center gap-1.5">
              <HelpCircle className="h-3.5 w-3.5 text-stone-400" />
              {testRun.skipped} skipped
            </span>
            <span className="flex items-center gap-1.5">
              <Clock className="h-3.5 w-3.5 text-stone-400" />
              {testRun.durationSec.toFixed(1)}s
            </span>
          </div>
        </div>
        {expanded ? (
          <ChevronDown className="h-4 w-4 text-stone-400" />
        ) : (
          <ChevronUp className="h-4 w-4 text-stone-400" />
        )}
      </button>

      {expanded && (
        <div id={panelId} className="border-t border-stone-100 px-5 pb-5">
          {hasFailures ? (
            <div className="mt-4">
              <div className="mb-3 flex items-center gap-2">
                <XCircle className="h-4 w-4 text-red-500" />
                <span className="text-xs font-semibold text-red-600 uppercase tracking-wider">
                  Failed Tests
                </span>
              </div>
              <div className="rounded-lg border border-red-200 bg-red-50 p-3">
                {failedTests.map((test) => (
                  <TestCaseRow key={test.id} test={test} onFileClick={onFileClick} />
                ))}
              </div>
            </div>
          ) : (
            <div className="mt-4 flex items-center gap-2 rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-3">
              <CheckCircle className="h-4 w-4 text-emerald-500" />
              <span className="text-sm text-emerald-700">All tests passed</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
