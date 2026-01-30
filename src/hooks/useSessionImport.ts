import { basename } from './basename';
import { mergeSanitizerHits, sanitizePayloadMessages } from './sessionUtils';
import { useCallback } from 'react';
import { open } from '@tauri-apps/plugin-dialog';
import { importAgentTraceFile } from '../core/repo/agentTrace';
import { linkSessionToCommit } from '../core/repo/sessionLinking';
import { deleteSessionLinkBySessionId, getSessionLinksForCommit } from '../core/repo/sessionLinking';
import { loadSessionExcerpts } from '../core/repo/sessions';
import { parseKimiContextJsonl } from '../core/repo/kimiAdapter';
import { sha256Hex } from '../core/security/hash';
import { redactSecrets } from '../core/security/redact';
import { sanitizeToolText, type ToolSanitizerHit } from '../core/security/toolSanitizer';
import { isoStampForFile } from './isoStampForFile';
import { readTextFile, writeNarrativeFile } from '../core/tauri/narrativeFs';
import { scanAgentTraceRecords } from '../core/repo/agentTrace';
import type { BranchViewModel, SessionExcerpt, SessionMessage, SessionTool } from '../core/types';

export interface UseSessionImportProps {
  repoRoot: string;
  repoId: number;
  model: BranchViewModel;
  setRepoState: (updater: (prev: BranchViewModel) => BranchViewModel) => void;
  setActionError: (error: string | null) => void;
}

export interface UseSessionImportReturn {
  importSession: () => Promise<void>;
  importKimiSession: () => Promise<void>;
  importAgentTrace: () => Promise<void>;
  unlinkSession: (sessionId: string) => Promise<void>;
}

/**
 * Hook for handling session imports (JSON, Kimi logs, Agent Traces).
 * Manages file import, sanitization, linking, and UI state updates.
 */
export function useSessionImport({
  repoRoot,
  repoId,
  model,
  setRepoState,
  setActionError
}: UseSessionImportProps): UseSessionImportReturn {
  const importSession = useCallback(async () => {
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
      await writeNarrativeFile(repoRoot, rel, JSON.stringify(wrapper, null, 2));

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

      // Link to best matching commit
      await linkSessionToCommit(repoId, sessionExcerpt);

      // Reload excerpts and update badges
      await refreshSessionBadges(repoRoot, repoId, model.timeline, setRepoState);
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, [repoRoot, repoId, model.timeline, setRepoState, setActionError]);

  const importKimiSession = useCallback(async () => {
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
      await writeNarrativeFile(repoRoot, rel, JSON.stringify(wrapper, null, 2));

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

      // Link to best matching commit
      await linkSessionToCommit(repoId, sessionExcerpt);

      // Reload excerpts and update badges
      await refreshSessionBadges(repoRoot, repoId, model.timeline, setRepoState);
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, [repoRoot, repoId, model.timeline, setRepoState, setActionError]);

  const importAgentTrace = useCallback(async () => {
    setActionError(null);

    try {
      const selected = await open({
        multiple: false,
        title: 'Import an Agent Trace JSON file',
        filters: [{ name: 'Agent Trace', extensions: ['json'] }]
      });

      if (!selected || Array.isArray(selected)) return;

      await importAgentTraceFile(repoRoot, repoId, selected);

      const commitShas = model.timeline.map((n) => n.id);
      const trace = await scanAgentTraceRecords(repoRoot, repoId, commitShas);

      setRepoState((prev) => applyTraceUpdate(prev, trace));
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : String(e));
    }
  }, [repoRoot, repoId, model, setRepoState, setActionError]);

  const unlinkSession = useCallback(
    async (sessionId: string) => {
      setActionError(null);

      try {
        await deleteSessionLinkBySessionId(repoId, sessionId);

        // Reload excerpts and update badges
        await refreshSessionBadges(repoRoot, repoId, model.timeline, setRepoState, true);
      } catch (e: unknown) {
        setActionError(e instanceof Error ? e.message : String(e));
      }
    },
    [repoRoot, repoId, model.timeline, setRepoState, setActionError]
  );

  return {
    importSession,
    importKimiSession,
    importAgentTrace,
    unlinkSession
  };
}

// Helper functions

function applyTraceUpdate(
  model: BranchViewModel,
  trace: Awaited<ReturnType<typeof scanAgentTraceRecords>>
): BranchViewModel {
  const timeline = model.timeline.map((node) => {
    const traceSummary = trace.byCommit[node.id];
    if (!traceSummary) return node;
    const existing = node.badges?.filter((b) => b.type !== 'trace') ?? [];
    
    const isUnknownOnly =
      traceSummary.unknownLines > 0 &&
      traceSummary.aiLines === 0 &&
      traceSummary.humanLines === 0 &&
      traceSummary.mixedLines === 0;
    
    const label = isUnknownOnly ? 'Unknown' : `AI ${traceSummary.aiPercent}%`;
    
    return {
      ...node,
      badges: [...existing, { type: 'trace' as const, label }]
    };
  });

  return {
    ...model,
    traceSummaries: { byCommit: trace.byCommit, byFileByCommit: trace.byFileByCommit },
    stats: {
      ...model.stats,
      prompts: trace.totals.conversations,
      responses: trace.totals.ranges
    },
    timeline
  };
}

async function refreshSessionBadges(
  repoRoot: string,
  repoId: number,
  timeline: Array<{ id: string }>,
  setRepoState: (updater: (prev: BranchViewModel) => BranchViewModel) => void,
  unlinkMode = false
) {
  // Reload excerpts (latest only)
  const sessionExcerpts = await loadSessionExcerpts(repoRoot, repoId, 1);

  // Fetch all session links for commits
  const commitShas = timeline.map((n) => n.id);
  const linksByCommit: Record<string, import('../core/repo/sessionLinking').SessionLink[]> = {};
  for (const sha of commitShas) {
    const links = await getSessionLinksForCommit(repoId, sha);
    if (links.length > 0) {
      linksByCommit[sha] = links;
    }
  }

  // Now update the state with the new badges
  setRepoState((prev) => {
    const existingBadges = prev.timeline.map((node) => {
      const links = linksByCommit[node.id];
      if (!links || links.length === 0) {
        // Remove session badges if no links (or if unlinking)
        if (unlinkMode) {
          return {
            ...node,
            badges: node.badges?.filter((b) => b.type !== 'session') ?? []
          };
        }
        return node;
      }
      const existing = node.badges?.filter((b) => b.type !== 'session') ?? [];
      return {
        ...node,
        badges: [...existing, { type: 'session' as const, label: `${links.length} session${links.length > 1 ? 's' : ''}` }]
      };
    });

    return {
      ...prev,
      sessionExcerpts,
      timeline: existingBadges
    };
  });
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
