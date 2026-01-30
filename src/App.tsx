import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { open as openExternal } from '@tauri-apps/plugin-shell';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { NearbyGridDemo } from './core/demo/nearbyGridDemo';
import {
  generateDerivedTraceRecord,
  getSessionLinkForCommit,
  getTraceRangesForCommitFile,
  importAgentTraceFile,
  ingestCodexOtelLogFile,
  scanAgentTraceRecords,
  writeGeneratedTraceRecord
} from './core/repo/agentTrace';
import { getCommitDiffForFile } from './core/repo/git';
import { getOrLoadCommitFiles, indexRepo, type RepoIndex } from './core/repo/indexer';
import { parseKimiContextJsonl } from './core/repo/kimiAdapter';
import {
  deleteSessionLinkBySessionId,
  getSessionLinksForCommit,
  linkSessionToCommit,
  type SessionLink
} from './core/repo/sessionLinking';
import { loadSessionExcerpts } from './core/repo/sessions';
import { defaultTraceConfig, saveTraceConfig } from './core/repo/traceConfig';
import { sha256Hex } from './core/security/hash';
import { redactSecrets } from './core/security/redact';
import { sanitizeToolText, type ToolSanitizerHit } from './core/security/toolSanitizer';
import { readTextFile, writeNarrativeFile } from './core/tauri/narrativeFs';
import { runOtlpSmokeTest, setActiveRepoRoot, setOtelReceiverEnabled } from './core/tauri/otelReceiver';
import { detectCodexOtelPromptExport } from './core/repo/codexConfig';
import type {
  BranchViewModel,
  FileChange,
  SessionExcerpt,
  SessionMessage,
  SessionTool,
  TraceCollectorConfig,
  TraceCollectorStatus
} from './core/types';
import { RepoEmptyState } from './ui/components/RepoEmptyState';
import { TopNav, type Mode } from './ui/components/TopNav';
import { BranchView } from './ui/views/BranchView';
import { SpeculateView } from './ui/views/SpeculateView';

/**
 * Simple LRU (Least Recently Used) cache with a maximum size limit.
 * Prevents unbounded memory growth by evicting the oldest entry when the limit is reached.
 */
class LRUCache<K, V> {
  private cache: Map<K, V>;
  private maxEntries: number;

  constructor(maxEntries: number = 100) {
    this.cache = new Map();
    this.maxEntries = maxEntries;
  }

  get(key: K): V | undefined {
    const value = this.cache.get(key);
    if (value !== undefined) {
      // Move to end (most recently used)
      this.cache.delete(key);
      this.cache.set(key, value);
    }
    return value;
  }

  set(key: K, value: V): void {
    // Remove existing key to update position
    if (this.cache.has(key)) {
      this.cache.delete(key);
    }
    // Evict oldest if at capacity
    else if (this.cache.size >= this.maxEntries) {
      const firstKey = this.cache.keys().next().value;
      if (firstKey !== undefined) {
        this.cache.delete(firstKey);
      }
    }
    // Add to end (most recently used)
    this.cache.set(key, value);
  }

  clear(): void {
    this.cache.clear();
  }

  get size(): number {
    return this.cache.size;
  }
}

type RepoState =
  | { status: 'idle' }
  | { status: 'loading'; path: string }
  | { status: 'ready'; path: string; model: BranchViewModel; repo: RepoIndex }
  | { status: 'error'; path?: string; message: string };

type OtelIngestNotification = {
  commitShas: string[];
  recordsWritten: number;
  dropped: number;
  issues: string[];
};

type TraceUpdateOptions = {
  traceStatus?: TraceCollectorStatus;
  traceConfig?: TraceCollectorConfig;
};

const CODEX_OTEL_DOCS_URL =
  'https://developers.openai.com/codex/config-advanced/#observability-and-telemetry';

function applyTraceUpdate(
  model: BranchViewModel,
  trace: Awaited<ReturnType<typeof scanAgentTraceRecords>>,
  options: TraceUpdateOptions = {}
): BranchViewModel {
  const timeline = model.timeline.map((node) => {
    const traceSummary = trace.byCommit[node.id];
    if (!traceSummary) return node;
    const existing = node.badges?.filter((b) => b.type !== 'trace') ?? [];
    return {
      ...node,
      badges: [...existing, { type: 'trace' as const, label: `AI ${traceSummary.aiPercent}%` }]
    };
  });

  return {
    ...model,
    traceSummaries: { byCommit: trace.byCommit, byFileByCommit: trace.byFileByCommit },
    traceStatus: options.traceStatus ?? model.traceStatus,
    traceConfig: options.traceConfig ?? model.traceConfig,
    stats: {
      ...model.stats,
      prompts: trace.totals.conversations,
      responses: trace.totals.ranges
    },
    timeline
  };
}

function basename(p: string) {
  const parts = p.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? p;
}

function isoStampForFile() {
  // 2026-01-27T13-29-25-123Z (safe for filenames)
  return new Date().toISOString().replace(/[:.]/g, '-');
}

function mergeSanitizerHits(target: ToolSanitizerHit[], incoming: ToolSanitizerHit[]) {
  for (const hit of incoming) {
    const existing = target.find((item) => item.type === hit.type);
    if (existing) {
      existing.count += hit.count;
    } else {
      target.push({ ...hit });
    }
  }
}

function isSessionMessageRecord(
  value: unknown
): value is { role: 'user' | 'assistant'; text: string; files?: string[] } {
  if (!value || typeof value !== 'object') return false;
  const record = value as Record<string, unknown>;
  const role = record.role;
  if (role !== 'user' && role !== 'assistant') return false;
  if (typeof record.text !== 'string') return false;
  if (record.files && !Array.isArray(record.files)) return false;
  return true;
}

function sanitizePayloadMessages(payload: unknown): { payload: unknown; hits: ToolSanitizerHit[] } {
  if (!payload || typeof payload !== 'object') return { payload, hits: [] };
  const record = payload as Record<string, unknown>;
  if (!Array.isArray(record.messages)) return { payload, hits: [] };

  const sanitizedMessages: Array<{ role: 'user' | 'assistant'; text: string; files?: string[] }> = [];
  const hits: ToolSanitizerHit[] = [];

  for (const entry of record.messages) {
    if (!isSessionMessageRecord(entry)) continue;
    const sanitized = sanitizeToolText(entry.text);
    mergeSanitizerHits(hits, sanitized.hits);
    sanitizedMessages.push({ ...entry, text: sanitized.sanitized });
  }

  if (sanitizedMessages.length === 0) return { payload, hits };

  return {
    payload: { ...record, messages: sanitizedMessages },
    hits
  };
}

export default function App() {
  const [mode, setMode] = useState<Mode>('demo');
  const [repoState, setRepoState] = useState<RepoState>({ status: 'idle' });
  const [actionError, setActionError] = useState<string | null>(null);
  const [codexPromptExport, setCodexPromptExport] = useState<{
    enabled: boolean | null;
    configPath: string | null;
  }>({ enabled: null, configPath: null });

  const diffCache = useRef(new LRUCache<string, string>(100)); // Max 100 cached diffs
  const repoStateRef = useRef(repoState);

  useEffect(() => {
    repoStateRef.current = repoState;
  }, [repoState]);

  const openRepo = useCallback(async () => {
    const selected = await open({ directory: true, multiple: false, title: 'Select a git repository folder' });
    if (!selected || Array.isArray(selected)) return;

    setMode('repo');
    setRepoState({ status: 'loading', path: selected });
    setActionError(null);

    try {
      const { model, repo } = await indexRepo(selected, 60);
      setRepoState({ status: 'ready', path: selected, model, repo });

      // Clear cache when loading a new repo to avoid stale data
      diffCache.current.clear();

      try {
        await setActiveRepoRoot(repo.root);
        const receiverEnabled = model.traceConfig?.codexOtelReceiverEnabled ?? false;
        await setOtelReceiverEnabled(receiverEnabled);
        const promptExport = await detectCodexOtelPromptExport();
        setCodexPromptExport(promptExport);
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    } catch (e: unknown) {
      setRepoState({
        status: 'error',
        path: selected,
        message: e instanceof Error ? e.message : String(e)
      });
    }
  }, []);

  useEffect(() => {
    let unlistenStatus: (() => void) | null = null;
    let unlistenIngest: (() => void) | null = null;

    const setup = async () => {
      unlistenStatus = await listen<TraceCollectorStatus>('otel-receiver-status', (event) => {
        const current = repoStateRef.current;
        if (current.status !== 'ready') return;
        setRepoState((prev) => {
          if (prev.status !== 'ready') return prev;
          return {
            ...prev,
            model: {
              ...prev.model,
              traceStatus: event.payload
            }
          };
        });
      });

      unlistenIngest = await listen<OtelIngestNotification>('otel-trace-ingested', async () => {
        const current = repoStateRef.current;
        if (current.status !== 'ready') return;

        try {
          const commitShas = current.model.timeline.map((node) => node.id);
          const trace = await scanAgentTraceRecords(current.repo.root, current.repo.repoId, commitShas);

          setRepoState((prev) => {
            if (prev.status !== 'ready') return prev;
            return {
              ...prev,
              model: applyTraceUpdate(prev.model, trace)
            };
          });
        } catch (e: unknown) {
          setActionError(e instanceof Error ? e.message : String(e));
        }
      });
    };

    void setup();

    return () => {
      if (unlistenStatus) unlistenStatus();
      if (unlistenIngest) unlistenIngest();
    };
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

      const sanitizedPayload = sanitizePayloadMessages(payload);
      const wrapper = {
        importedAtISO: new Date().toISOString(),
        sourceBasename: basename(selected),
        sha256: sha,
        redactions: hits,
        toolSanitizer: sanitizedPayload.hits,
        payload: sanitizedPayload.payload
      };

      const rel = `sessions/imported/${isoStampForFile()}_${sha.slice(0, 8)}.json`;
      await writeNarrativeFile(repoState.repo.root, rel, JSON.stringify(wrapper, null, 2));

      // Extract messages for linking
      let messages: SessionMessage[];
      if (sanitizedPayload.payload &&
        typeof sanitizedPayload.payload === 'object' &&
        'messages' in sanitizedPayload.payload &&
        Array.isArray(sanitizedPayload.payload.messages)) {
        messages = sanitizedPayload.payload.messages
          .map((m: unknown, idx: number): SessionMessage | null => {
            if (!isSessionMessageRecord(m)) return null;
            return {
              id: `${sha.slice(0, 8)}-${idx}`,
              role: m.role,
              text: m.text,
              files: m.files
            };
          })
          .filter((m): m is SessionMessage => m !== null);
      } else {
        messages = [{ id: `${sha.slice(0, 8)}-0`, role: 'user' as const, text: redacted }];
      }

      // Create session excerpt for linking
      const sessionExcerpt: SessionExcerpt = {
        id: sha,
        tool: (sanitizedPayload.payload &&
          typeof sanitizedPayload.payload === 'object' &&
          'tool' in sanitizedPayload.payload &&
          typeof sanitizedPayload.payload.tool === 'string'
          ? sanitizedPayload.payload.tool
          : 'unknown') as SessionTool,
        durationMin: undefined,
        messages
      };

      // Link to best matching commit using the linking algorithm
      await linkSessionToCommit(repoState.repo.repoId, sessionExcerpt);

      // Reload excerpts (latest only)
      const sessionExcerpts = await loadSessionExcerpts(repoState.repo.root, repoState.repo.repoId, 1);

      // Get session links for all commits to update badges
      const commitShas = repoState.model.timeline.map((n) => n.id);
      const linksByCommit: Record<string, SessionLink[]> = {};
      for (const sha of commitShas) {
        const links = await getSessionLinksForCommit(repoState.repo.repoId, sha);
        if (links.length > 0) {
          linksByCommit[sha] = links;
        }
      }

      setRepoState((prev) => {
        if (prev.status !== 'ready') return prev;
        return {
          ...prev,
          model: {
            ...prev.model,
            sessionExcerpts,
            timeline: prev.model.timeline.map((node) => {
              const links = linksByCommit[node.id];
              if (!links || links.length === 0) return node;
              const existing = node.badges?.filter((b) => b.type !== 'session') ?? [];
              return {
                ...node,
                badges: [...existing, { type: 'session', label: `${links.length} session${links.length > 1 ? 's' : ''}` }]
              };
            })
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
          model: applyTraceUpdate(prev.model, trace)
        };
      });
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, [repoState]);

  const importKimiSession = useCallback(async () => {
    if (repoState.status !== 'ready') return;
    setActionError(null);

    try {
      const selected = await open({
        multiple: false,
        title: 'Import a Kimi CLI log (context.jsonl)',
        filters: [{ name: 'JSON Lines', extensions: ['jsonl', 'json'] }]
      });

      if (!selected || Array.isArray(selected)) return;

      const raw = await readTextFile(selected);
      const { redacted, hits } = redactSecrets(raw);
      const sha = await sha256Hex(redacted);
      const parsed = parseKimiContextJsonl(redacted);

      if (parsed.messages.length === 0) {
        throw new Error('No readable messages found in the Kimi context log.');
      }

      const sanitizedMessages = parsed.messages.map((message) => {
        const sanitized = sanitizeToolText(message.text);
        return {
          message: {
            role: message.role,
            text: sanitized.sanitized,
            files: message.files
          },
          hits: sanitized.hits
        };
      });
      const toolHits: ToolSanitizerHit[] = [];
      for (const entry of sanitizedMessages) {
        mergeSanitizerHits(toolHits, entry.hits);
      }

      const payload = {
        tool: 'kimi',
        modelId: parsed.modelId,
        messages: sanitizedMessages.map((entry) => entry.message)
      };

      const wrapper = {
        importedAtISO: new Date().toISOString(),
        sourceBasename: basename(selected),
        sha256: sha,
        sessionId: `kimi:${sha}`,
        redactions: hits,
        toolSanitizer: toolHits,
        payload
      };

      const rel = `sessions/imported/${isoStampForFile()}_${sha.slice(0, 8)}_kimi.json`;
      await writeNarrativeFile(repoState.repo.root, rel, JSON.stringify(wrapper, null, 2));

      // Create session excerpt for linking
      const sessionExcerpt: SessionExcerpt = {
        id: `kimi:${sha}`,
        tool: 'kimi' as SessionTool,
        durationMin: undefined,
        messages: sanitizedMessages.map((entry, idx) => ({
          id: `kimi:${sha}-${idx}`,
          ...entry.message
        }))
      };

      // Link to best matching commit using the linking algorithm
      await linkSessionToCommit(repoState.repo.repoId, sessionExcerpt);

      // Reload excerpts (latest only)
      const sessionExcerpts = await loadSessionExcerpts(repoState.repo.root, repoState.repo.repoId, 1);

      // Get session links for all commits to update badges
      const commitShas = repoState.model.timeline.map((n) => n.id);
      const linksByCommit: Record<string, SessionLink[]> = {};
      for (const sha of commitShas) {
        const links = await getSessionLinksForCommit(repoState.repo.repoId, sha);
        if (links.length > 0) {
          linksByCommit[sha] = links;
        }
      }

      setRepoState((prev) => {
        if (prev.status !== 'ready') return prev;
        return {
          ...prev,
          model: {
            ...prev.model,
            sessionExcerpts,
            timeline: prev.model.timeline.map((node) => {
              const links = linksByCommit[node.id];
              if (!links || links.length === 0) return node;
              const existing = node.badges?.filter((b) => b.type !== 'session') ?? [];
              return {
                ...node,
                badges: [...existing, { type: 'session', label: `${links.length} session${links.length > 1 ? 's' : ''}` }]
              };
            })
          }
        };
      });

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

  const updateCodexOtelPath = useCallback(
    async (path: string) => {
      if (repoState.status !== 'ready') return;
      setActionError(null);

      try {
        const baseConfig = repoState.model.traceConfig ?? defaultTraceConfig();
        const nextConfig = { ...baseConfig, codexOtelLogPath: path };
        await saveTraceConfig(repoState.repo.root, nextConfig);
        const ingest = await ingestCodexOtelLogFile({
          repoRoot: repoState.repo.root,
          repoId: repoState.repo.repoId,
          logPath: path
        });
        const commitShas = repoState.model.timeline.map((n) => n.id);
        const trace = await scanAgentTraceRecords(repoState.repo.root, repoState.repo.repoId, commitShas);

        setRepoState((prev) => {
          if (prev.status !== 'ready') return prev;
          return {
            ...prev,
            model: applyTraceUpdate(prev.model, trace, {
              traceStatus: ingest.status,
              traceConfig: nextConfig
            })
          };
        });
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [repoState]
  );

  const updateCodexOtelReceiverEnabled = useCallback(
    async (enabled: boolean) => {
      if (repoState.status !== 'ready') return;
      setActionError(null);

      try {
        const baseConfig = repoState.model.traceConfig ?? defaultTraceConfig();
        const nextConfig = { ...baseConfig, codexOtelReceiverEnabled: enabled };
        await saveTraceConfig(repoState.repo.root, nextConfig);
        await setOtelReceiverEnabled(enabled);

        setRepoState((prev) => {
          if (prev.status !== 'ready') return prev;
          return {
            ...prev,
            model: {
              ...prev.model,
              traceConfig: nextConfig,
              traceStatus: enabled
                ? prev.model.traceStatus
                : { state: 'inactive', message: 'Codex OTel receiver disabled' }
            }
          };
        });
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [repoState]
  );

  const openCodexOtelDocs = useCallback(async () => {
    try {
      await openExternal(CODEX_OTEL_DOCS_URL);
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const unlinkSession = useCallback(
    async (sessionId: string) => {
      if (repoState.status !== 'ready') return;
      setActionError(null);

      try {
        await deleteSessionLinkBySessionId(repoState.repo.repoId, sessionId);

        // Reload excerpts (latest only)
        const sessionExcerpts = await loadSessionExcerpts(repoState.repo.root, repoState.repo.repoId, 1);

        // Get session links for all commits to update badges
        const commitShas = repoState.model.timeline.map((n) => n.id);
        const linksByCommit: Record<string, SessionLink[]> = {};
        for (const sha of commitShas) {
          const links = await getSessionLinksForCommit(repoState.repo.repoId, sha);
          if (links.length > 0) {
            linksByCommit[sha] = links;
          }
        }

        setRepoState((prev) => {
          if (prev.status !== 'ready') return prev;
          return {
            ...prev,
            model: {
              ...prev.model,
              sessionExcerpts,
              timeline: prev.model.timeline.map((node) => {
                const links = linksByCommit[node.id];
                if (!links || links.length === 0) {
                  // Remove session badges if no links
                  return {
                    ...node,
                    badges: node.badges?.filter((b) => b.type !== 'session') ?? []
                  };
                }
                const existing = node.badges?.filter((b) => b.type !== 'session') ?? [];
                return {
                  ...node,
                  badges: [...existing, { type: 'session', label: `${links.length} session${links.length > 1 ? 's' : ''}` }]
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
            model: applyTraceUpdate(prev.model, trace)
          };
        });
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [repoState]
  );

  const runOtlpSmokeTestHandler = useCallback(
    async (nodeId: string, files: FileChange[]) => {
      if (repoState.status !== 'ready') return;
      setActionError(null);

      try {
        if (files.length === 0) {
          throw new Error('Select a commit with changed files to run the smoke test.');
        }

        await runOtlpSmokeTest(
          repoState.repo.root,
          nodeId,
          files.map((file) => file.path)
        );

        const commitShas = repoState.model.timeline.map((n) => n.id);
        const trace = await scanAgentTraceRecords(repoState.repo.root, repoState.repo.repoId, commitShas);

        setRepoState((prev) => {
          if (prev.status !== 'ready') return prev;
          return {
            ...prev,
            model: applyTraceUpdate(prev.model, trace)
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
        onImportKimiSession={importKimiSession}
        onImportAgentTrace={importAgentTrace}
        importEnabled={importEnabled}
      />



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
            onRunOtlpSmokeTest={runOtlpSmokeTestHandler}
            onUpdateCodexOtelPath={updateCodexOtelPath}
            onToggleCodexOtelReceiver={updateCodexOtelReceiverEnabled}
            onOpenCodexOtelDocs={openCodexOtelDocs}
            codexPromptExport={codexPromptExport}
            onUnlinkSession={unlinkSession}
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
