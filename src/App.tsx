import { useCallback, useMemo, useRef, useState } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import type { BranchViewModel, FileChange } from './core/types';
import { NearbyGridDemo } from './core/demo/nearbyGridDemo';
import { redactSecrets } from './core/security/redact';
import { sha256Hex } from './core/security/hash';
import { getCommitDiffForFile } from './core/repo/git';
import { getOrLoadCommitFiles, indexRepo, type RepoIndex } from './core/repo/indexer';
import { loadSessionExcerpts } from './core/repo/sessions';
import { readTextFile, writeNarrativeFile } from './core/tauri/narrativeFs';
import {
  generateDerivedTraceRecord,
  getSessionLinkForCommit,
  getTraceRangesForCommitFile,
  importAgentTraceFile,
  scanAgentTraceRecords,
  writeGeneratedTraceRecord
} from './core/repo/agentTrace';
import { TopNav, type Mode } from './ui/components/TopNav';
import { BranchView } from './ui/views/BranchView';
import { SpeculateView } from './ui/views/SpeculateView';

type RepoState =
  | { status: 'idle' }
  | { status: 'loading'; path: string }
  | { status: 'ready'; path: string; model: BranchViewModel; repo: RepoIndex }
  | { status: 'error'; path?: string; message: string };

function basename(p: string) {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

function isoStampForFile() {
  // 2026-01-27T13-29-25-123Z (safe for filenames)
  return new Date().toISOString().replace(/[:.]/g, '-');
}

export default function App() {
  const [mode, setMode] = useState<Mode>('demo');
  const [repoState, setRepoState] = useState<RepoState>({ status: 'idle' });
  const [actionError, setActionError] = useState<string | null>(null);

  const diffCache = useRef<Map<string, string>>(new Map());

  const openRepo = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false, title: 'Select a git repository folder' });
    if (!selected || Array.isArray(selected)) return;

    setMode('repo');
    setRepoState({ status: 'loading', path: selected });
    setActionError(null);

    try {
      const { model, repo } = await indexRepo(selected, 60);
      setRepoState({ status: 'ready', path: selected, model, repo });
    } catch (e: unknown) {
      setRepoState({
        status: 'error',
        path: selected,
        message: e instanceof Error ? e.message : String(e)
      });
    }
  }, []);

  const importSession = useCallback(async () => {
    if (repoState.status !== 'ready') return;
    setActionError(null);

    try {
      const selected = await open({
        multiple: false,
        title: 'Import a session JSON file',
        filters: [{ name: 'JSON', extensions: ['json'] }]
      });

      if (!selected || Array.isArray(selected)) return;

      const raw = await readTextFile(selected);
      const { redacted, hits } = redactSecrets(raw);
      const sha = await sha256Hex(redacted);

      let payload: unknown;
      try {
        payload = JSON.parse(redacted);
      } catch {
        payload = {
          tool: 'unknown',
          messages: [{ role: 'user', text: redacted }]
        };
      }

      const wrapper = {
        importedAtISO: new Date().toISOString(),
        sourceBasename: basename(selected),
        sha256: sha,
        redactions: hits,
        payload
      };

      const rel = `sessions/imported/${isoStampForFile()}_${sha.slice(0, 8)}.json`;
      await writeNarrativeFile(repoState.repo.root, rel, JSON.stringify(wrapper, null, 2));

      // Reload excerpts (latest only)
      const sessionExcerpts = await loadSessionExcerpts(repoState.repo.root, 1);

      setRepoState((prev) => {
        if (prev.status !== 'ready') return prev;
        return {
          ...prev,
          model: {
            ...prev.model,
            sessionExcerpts
          }
        };
      });
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, [repoState]);

  const importAgentTrace = useCallback(async () => {
    if (repoState.status !== 'ready') return;
    setActionError(null);

    try {
      const selected = await open({
        multiple: false,
        title: 'Import an Agent Trace JSON file',
        filters: [{ name: 'Agent Trace', extensions: ['json'] }]
      });

      if (!selected || Array.isArray(selected)) return;

      await importAgentTraceFile(repoState.repo.root, repoState.repo.repoId, selected);

      const commitShas = repoState.model.timeline.map((n) => n.id);
      const trace = await scanAgentTraceRecords(repoState.repo.root, repoState.repo.repoId, commitShas);

      setRepoState((prev) => {
        if (prev.status !== 'ready') return prev;
        return {
          ...prev,
          model: {
            ...prev.model,
            traceSummaries: { byCommit: trace.byCommit, byFile: trace.byFile },
            stats: {
              ...prev.model.stats,
              prompts: trace.totals.conversations,
              responses: trace.totals.ranges
            },
            timeline: prev.model.timeline.map((node) => {
              const traceSummary = trace.byCommit[node.id];
              if (!traceSummary) return node;
              const existing = node.badges?.filter((b) => b.type !== 'trace') ?? [];
              return {
                ...node,
                badges: [...existing, { type: 'trace', label: `AI ${traceSummary.aiPercent}%` }]
              };
            })
          }
        };
      });
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, [repoState]);

  const model: BranchViewModel | null = useMemo(() => {
    if (mode === 'demo') return NearbyGridDemo;
    if (mode === 'repo' && repoState.status === 'ready') return repoState.model;
    return null;
  }, [mode, repoState]);

  const repoPath = useMemo(() => {
    if (repoState.status === 'ready') return repoState.repo.root;
    if (repoState.status === 'loading') return repoState.path;
    return null;
  }, [repoState]);

  const loadFilesForNode = useCallback(
    async (nodeId: string): Promise<FileChange[]> => {
      if (!model) return [];

      if (model.source === 'demo') {
        return model.filesChanged ?? [];
      }

      if (repoState.status !== 'ready') return [];
      return await getOrLoadCommitFiles(repoState.repo, nodeId);
    },
    [model, repoState]
  );

  const loadDiffForFile = useCallback(
    async (nodeId: string, filePath: string): Promise<string> => {
      if (!model) return '';

      if (model.source === 'demo') {
        return model.diffsByFile?.[filePath] ?? '(no demo diff for this file)';
      }

      if (repoState.status !== 'ready') return '';

      const cacheKey = `${nodeId}:${filePath}`;
      const cached = diffCache.current.get(cacheKey);
      if (cached) return cached;

      const diff = await getCommitDiffForFile(repoState.repo.root, nodeId, filePath);
      diffCache.current.set(cacheKey, diff);
      return diff;
    },
    [model, repoState]
  );

  const loadTraceRangesForFile = useCallback(
    async (nodeId: string, filePath: string) => {
      if (!model) return [];
      if (model.source === 'demo') return [];
      if (repoState.status !== 'ready') return [];
      return await getTraceRangesForCommitFile(repoState.repo.repoId, nodeId, filePath);
    },
    [model, repoState]
  );

  const exportAgentTrace = useCallback(
    async (nodeId: string, files: FileChange[]) => {
      if (repoState.status !== 'ready') return;
      setActionError(null);

      try {
        const sessionId = await getSessionLinkForCommit(repoState.repo.repoId, nodeId);
        const record = await generateDerivedTraceRecord({
          repoRoot: repoState.repo.root,
          commitSha: nodeId,
          files,
          sessionId
        });
        await writeGeneratedTraceRecord(repoState.repo.root, record);

        const commitShas = repoState.model.timeline.map((n) => n.id);
        const trace = await scanAgentTraceRecords(repoState.repo.root, repoState.repo.repoId, commitShas);

        setRepoState((prev) => {
          if (prev.status !== 'ready') return prev;
          return {
            ...prev,
            model: {
              ...prev.model,
              traceSummaries: { byCommit: trace.byCommit, byFile: trace.byFile },
              stats: {
                ...prev.model.stats,
                prompts: trace.totals.conversations,
                responses: trace.totals.ranges
              },
              timeline: prev.model.timeline.map((node) => {
                const traceSummary = trace.byCommit[node.id];
                if (!traceSummary) return node;
                const existing = node.badges?.filter((b) => b.type !== 'trace') ?? [];
                return {
                  ...node,
                  badges: [...existing, { type: 'trace', label: `AI ${traceSummary.aiPercent}%` }]
                };
              })
            }
          };
        });
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [repoState]
  );

  const importEnabled = mode === 'repo' && repoState.status === 'ready';

  return (
    <div className="flex h-full flex-col bg-[#f5f5f4] text-stone-800">
      <TopNav
        mode={mode}
        onModeChange={setMode}
        repoPath={repoPath}
        onOpenRepo={openRepo}
        onImportSession={importSession}
        onImportAgentTrace={importAgentTrace}
        importEnabled={importEnabled}
      />

      {actionError ? (
        <div className="border-b border-red-200 bg-red-50 px-4 py-2 text-sm text-red-700">
          {actionError}
        </div>
      ) : null}

      <div className="flex-1 overflow-hidden">
        {mode === 'speculate' ? (
          <SpeculateView />
        ) : mode === 'repo' && repoState.status === 'loading' ? (
          <div className="p-8 text-sm text-stone-500">Indexing repo…</div>
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
        ) : model ? (
          <BranchView
            model={model}
            loadFilesForNode={loadFilesForNode}
            loadDiffForFile={loadDiffForFile}
            loadTraceRangesForFile={loadTraceRangesForFile}
            onExportAgentTrace={exportAgentTrace}
          />
        ) : (
          <div className="p-8 text-sm text-stone-500">Pick a mode.</div>
        )}
      </div>
    </div>
  );
}
