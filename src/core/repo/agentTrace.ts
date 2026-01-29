import type {
  FileChange,
  TraceCommitSummary,
  TraceContributorType,
  TraceFileSummary,
  TraceRange,
  TraceRecord
} from '../types';
import { redactSecrets } from '../security/redact';
import { getDb } from './db';
import { listNarrativeFiles, readNarrativeFile, readTextFile, writeNarrativeFile } from '../tauri/narrativeFs';
import { getCommitDiffForFile } from './git';

const TRACE_EXTENSION = '.agent-trace.json';
const TRACE_DIR = 'trace';
const TRACE_GENERATED_DIR = 'trace/generated';

const CONTRIBUTOR_TYPES: TraceContributorType[] = ['human', 'ai', 'mixed', 'unknown'];

export type TraceScanResult = {
  byCommit: Record<string, TraceCommitSummary>;
  byFile: Record<string, TraceFileSummary>;
  totals: { conversations: number; ranges: number };
};

export type TraceImportResult = {
  recordId: string;
  storedPath: string;
  redactions: { type: string; count: number }[];
};

function isContributorType(value: string): value is TraceContributorType {
  return CONTRIBUTOR_TYPES.includes(value as TraceContributorType);
}

function normalizeContributorType(value: string | undefined): TraceContributorType {
  if (!value) return 'unknown';
  if (isContributorType(value)) return value;
  return 'unknown';
}

function safeString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function parseTraceRecord(raw: string): TraceRecord | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }

  if (!parsed || typeof parsed !== 'object') return null;
  const rawRecord = parsed as {
    id?: string;
    version?: string;
    timestamp?: string;
    vcs?: { type?: string; revision?: string };
    tool?: { name?: string; version?: string };
    files?: Array<{
      path?: string;
      conversations?: Array<{
        url?: string;
        contributor?: { type?: string; model_id?: string; modelId?: string };
        ranges?: Array<{
          start_line?: number;
          end_line?: number;
          content_hash?: string;
          contributor?: { type?: string; model_id?: string; modelId?: string };
        }>;
        related?: Array<{ type?: string; url?: string }>;
      }>;
    }>;
    metadata?: Record<string, unknown>;
  };

  if (!rawRecord.id || !rawRecord.version || !rawRecord.timestamp || !rawRecord.files) return null;
  if (!rawRecord.vcs || rawRecord.vcs.type !== 'git' || !rawRecord.vcs.revision) return null;
  if (!Array.isArray(rawRecord.files)) return null;

  const files = rawRecord.files.map((file) => ({
    path: file.path ?? '',
    conversations: (file.conversations ?? []).map((conversation) => ({
      url: conversation.url,
      contributor: conversation.contributor
        ? {
            type: normalizeContributorType(conversation.contributor.type),
            modelId: safeString(conversation.contributor.modelId ?? conversation.contributor.model_id)
          }
        : undefined,
      ranges: (conversation.ranges ?? []).map((range) => ({
        startLine: range.start_line ?? 1,
        endLine: range.end_line ?? range.start_line ?? 1,
        contentHash: range.content_hash,
        contributor: range.contributor
          ? {
              type: normalizeContributorType(range.contributor.type),
              modelId: safeString(range.contributor.modelId ?? range.contributor.model_id)
            }
          : undefined
      })),
      related: (conversation.related ?? []).flatMap((rel) =>
        rel.type && rel.url ? [{ type: rel.type, url: rel.url }] : []
      )
    }))
  }));

  return {
    id: rawRecord.id,
    version: rawRecord.version,
    timestamp: rawRecord.timestamp,
    vcs: { type: 'git', revision: rawRecord.vcs.revision },
    tool: rawRecord.tool,
    files,
    metadata: rawRecord.metadata
  };
}

function toLineCount(start: number, end: number) {
  return Math.max(0, end - start + 1);
}

function toAiPercent(aiLines: number, total: number) {
  if (total <= 0) return 0;
  return Math.round((aiLines / total) * 100);
}

async function recordExists(repoId: number, recordId: string) {
  const db = await getDb();
  const rows = await db.select<{ id: string }[]>(
    'SELECT id FROM trace_records WHERE repo_id = $1 AND id = $2',
    [repoId, recordId]
  );
  return Boolean(rows?.[0]?.id);
}

export async function ingestTraceRecord(repoId: number, record: TraceRecord): Promise<void> {
  if (await recordExists(repoId, record.id)) return;
  const db = await getDb();

  await db.execute('BEGIN');
  try {
    await db.execute(
      `INSERT OR IGNORE INTO trace_records
        (id, repo_id, version, timestamp, vcs_type, revision, tool_name, tool_version, metadata_json)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)`,
      [
        record.id,
        repoId,
        record.version,
        record.timestamp,
        record.vcs.type,
        record.vcs.revision,
        record.tool?.name ?? null,
        record.tool?.version ?? null,
        record.metadata ? JSON.stringify(record.metadata) : null
      ]
    );

    for (const file of record.files) {
      await db.execute('INSERT INTO trace_files (record_id, path) VALUES ($1, $2)', [
        record.id,
        file.path
      ]);

      const fileRow = await db.select<{ id: number }[]>(
        'SELECT id FROM trace_files WHERE record_id = $1 AND path = $2 ORDER BY id DESC LIMIT 1',
        [record.id, file.path]
      );
      const fileId = fileRow?.[0]?.id;
      if (!fileId) continue;

      for (const conversation of file.conversations ?? []) {
        const convContributorType = normalizeContributorType(conversation.contributor?.type);
        const convModelId = safeString(conversation.contributor?.modelId);

        await db.execute(
          'INSERT INTO trace_conversations (file_id, url, contributor_type, model_id) VALUES ($1, $2, $3, $4)',
          [fileId, conversation.url ?? null, convContributorType, convModelId ?? null]
        );

        const convRow = await db.select<{ id: number }[]>(
          'SELECT id FROM trace_conversations WHERE file_id = $1 ORDER BY id DESC LIMIT 1',
          [fileId]
        );
        const conversationId = convRow?.[0]?.id;
        if (!conversationId) continue;

        for (const range of conversation.ranges ?? []) {
          const rangeContributorType = normalizeContributorType(range.contributor?.type ?? convContributorType);
          const rangeModelId = safeString(range.contributor?.modelId ?? convModelId);

          await db.execute(
            `INSERT INTO trace_ranges
              (conversation_id, start_line, end_line, content_hash, contributor_type, model_id)
             VALUES ($1, $2, $3, $4, $5, $6)`,
            [
              conversationId,
              range.startLine,
              range.endLine,
              range.contentHash ?? null,
              rangeContributorType,
              rangeModelId ?? null
            ]
          );
        }
      }
    }

    await db.execute('COMMIT');
  } catch (error) {
    await db.execute('ROLLBACK');
    throw error;
  }
}

export async function scanAgentTraceRecords(
  repoRoot: string,
  repoId: number,
  commitShas: string[]
): Promise<TraceScanResult> {
  const files = await listNarrativeFiles(repoRoot, TRACE_DIR);
  const traceFiles = files.filter((p) => p.endsWith(TRACE_EXTENSION));

  for (const rel of traceFiles) {
    const raw = await readNarrativeFile(repoRoot, rel);
    const parsed = parseTraceRecord(raw);
    if (!parsed) continue;
    if (parsed.vcs.type !== 'git') continue;

    await ingestTraceRecord(repoId, parsed);
  }

  const byCommit: Record<string, TraceCommitSummary> = {};
  const byFile: Record<string, TraceFileSummary> = {};
  let totalConversations = 0;
  let totalRanges = 0;

  for (const sha of commitShas) {
    const summary = await getTraceSummaryForCommit(repoId, sha);
    if (!summary) continue;

    byCommit[sha] = summary.commit;
    Object.assign(byFile, summary.files);
    totalConversations += summary.totals.conversations;
    totalRanges += summary.totals.ranges;
  }

  return {
    byCommit,
    byFile,
    totals: { conversations: totalConversations, ranges: totalRanges }
  };
}

export async function importAgentTraceFile(
  repoRoot: string,
  repoId: number,
  absPath: string
): Promise<TraceImportResult> {
  const raw = await readTextFile(absPath);
  const redacted = redactSecrets(raw);
  const parsed = parseTraceRecord(redacted.redacted);

  if (!parsed) {
    throw new Error('Invalid Agent Trace record');
  }
  if (parsed.vcs.type !== 'git') {
    throw new Error('Only git-based Agent Trace records are supported');
  }

  const fileName = `${parsed.timestamp.replace(/[:.]/g, '-')}_${parsed.id}${TRACE_EXTENSION}`;
  const rel = `${TRACE_DIR}/${fileName}`;
  await writeNarrativeFile(repoRoot, rel, JSON.stringify(parsed, null, 2));
  await ingestTraceRecord(repoId, parsed);

  return {
    recordId: parsed.id,
    storedPath: rel,
    redactions: redacted.hits
  };
}

export async function getTraceRangesForCommitFile(
  repoId: number,
  commitSha: string,
  filePath: string
): Promise<TraceRange[]> {
  const db = await getDb();
  const rows = await db.select<
    Array<{ start_line: number; end_line: number; content_hash: string | null; contributor_type: string; model_id: string | null }>
  >(
    `SELECT tr.start_line, tr.end_line, tr.content_hash, tr.contributor_type, tr.model_id
     FROM trace_records r
     JOIN trace_files tf ON tf.record_id = r.id
     JOIN trace_conversations tc ON tc.file_id = tf.id
     JOIN trace_ranges tr ON tr.conversation_id = tc.id
     WHERE r.repo_id = $1 AND r.revision = $2 AND tf.path = $3
     ORDER BY tr.start_line ASC`,
    [repoId, commitSha, filePath]
  );

  return rows.map((row) => ({
    startLine: row.start_line,
    endLine: row.end_line,
    contentHash: row.content_hash ?? undefined,
    contributor: {
      type: normalizeContributorType(row.contributor_type),
      modelId: row.model_id ?? undefined
    }
  }));
}

export async function getSessionLinkForCommit(repoId: number, commitSha: string): Promise<string | null> {
  const db = await getDb();
  const rows = await db.select<Array<{ session_id: string }>>(
    'SELECT session_id FROM session_links WHERE repo_id = $1 AND commit_sha = $2 ORDER BY created_at DESC LIMIT 1',
    [repoId, commitSha]
  );
  return rows?.[0]?.session_id ?? null;
}

type TraceSummaryResult = {
  commit: TraceCommitSummary;
  files: Record<string, TraceFileSummary>;
  totals: { conversations: number; ranges: number };
};

async function getTraceSummaryForCommit(repoId: number, commitSha: string): Promise<TraceSummaryResult | null> {
  const db = await getDb();
  const rows = await db.select<
    Array<{ path: string; start_line: number; end_line: number; contributor_type: string; model_id: string | null }>
  >(
    `SELECT tf.path, tr.start_line, tr.end_line, tr.contributor_type, tr.model_id
     FROM trace_records r
     JOIN trace_files tf ON tf.record_id = r.id
     JOIN trace_conversations tc ON tc.file_id = tf.id
     JOIN trace_ranges tr ON tr.conversation_id = tc.id
     WHERE r.repo_id = $1 AND r.revision = $2`,
    [repoId, commitSha]
  );

  if (!rows || rows.length === 0) return null;

  const totalsRow = await db.select<Array<{ conversations: number; ranges: number }>>(
    `SELECT COUNT(DISTINCT tc.id) as conversations, COUNT(tr.id) as ranges\n     FROM trace_records r\n     JOIN trace_files tf ON tf.record_id = r.id\n     JOIN trace_conversations tc ON tc.file_id = tf.id\n     JOIN trace_ranges tr ON tr.conversation_id = tc.id\n     WHERE r.repo_id = $1 AND r.revision = $2`,
    [repoId, commitSha]
  );

  const fileMap: Record<string, TraceFileSummary> = {};
  const modelIds = new Set<string>();
  let aiLines = 0;
  let humanLines = 0;
  let mixedLines = 0;
  let unknownLines = 0;

  for (const row of rows) {
    const type = normalizeContributorType(row.contributor_type);
    const count = toLineCount(row.start_line, row.end_line);

    if (!fileMap[row.path]) {
      fileMap[row.path] = {
        path: row.path,
        aiLines: 0,
        humanLines: 0,
        mixedLines: 0,
        unknownLines: 0,
        aiPercent: 0
      };
    }

    if (type === 'ai') {
      aiLines += count;
      fileMap[row.path].aiLines += count;
    } else if (type === 'human') {
      humanLines += count;
      fileMap[row.path].humanLines += count;
    } else if (type === 'mixed') {
      mixedLines += count;
      fileMap[row.path].mixedLines += count;
    } else {
      unknownLines += count;
      fileMap[row.path].unknownLines += count;
    }

    if (row.model_id) modelIds.add(row.model_id);
  }

  for (const file of Object.values(fileMap)) {
    const total = file.aiLines + file.humanLines + file.mixedLines + file.unknownLines;
    file.aiPercent = toAiPercent(file.aiLines, total);
  }

  const totalLines = aiLines + humanLines + mixedLines + unknownLines;

  return {
    commit: {
      commitSha,
      aiLines,
      humanLines,
      mixedLines,
      unknownLines,
      aiPercent: toAiPercent(aiLines, totalLines),
      modelIds: Array.from(modelIds)
    },
    files: fileMap,
    totals: {
      conversations: totalsRow?.[0]?.conversations ?? 0,
      ranges: totalsRow?.[0]?.ranges ?? rows.length
    }
  };
}

function parseDiffAddedRanges(diffText: string): Array<{ start: number; end: number }> {
  const ranges: Array<{ start: number; end: number }> = [];
  let currentLine = 0;
  let inHunk = false;
  let activeStart: number | null = null;

  const hunkRegex = /^@@\s+-(\d+),(\d+)\s+\+(\d+),(\d+)\s+@@/;

  for (const line of diffText.split(/\r?\n/)) {
    const match = hunkRegex.exec(line);
    if (match) {
      currentLine = Number(match[3]);
      inHunk = true;
      activeStart = null;
      continue;
    }

    if (!inHunk) continue;

    if (line.startsWith('+') && !line.startsWith('+++')) {
      if (activeStart === null) activeStart = currentLine;
      currentLine += 1;
    } else if (line.startsWith('-') && !line.startsWith('---')) {
      if (activeStart !== null) {
        ranges.push({ start: activeStart, end: currentLine - 1 });
        activeStart = null;
      }
    } else {
      if (activeStart !== null) {
        ranges.push({ start: activeStart, end: currentLine - 1 });
        activeStart = null;
      }
      currentLine += 1;
    }
  }

  if (activeStart !== null) {
    ranges.push({ start: activeStart, end: currentLine - 1 });
  }

  return ranges;
}

function buildDerivedRangeConversations(
  filePath: string,
  diffText: string
): TraceRecord['files'][number] | null {
  const ranges = parseDiffAddedRanges(diffText);
  if (ranges.length === 0) return null;

  return {
    path: filePath,
    conversations: [
      {
        contributor: { type: 'ai' },
        ranges: ranges.map((r) => ({ startLine: r.start, endLine: r.end }))
      }
    ]
  };
}

export async function generateDerivedTraceRecord(options: {
  repoRoot: string;
  commitSha: string;
  files: FileChange[];
  sessionId?: string | null;
}): Promise<TraceRecord> {
  const { repoRoot, commitSha, files, sessionId } = options;
  const fileEntries: TraceRecord['files'] = [];

  for (const file of files) {
    const diff = await getCommitDiffForFile(repoRoot, commitSha, file.path);
    const entry = buildDerivedRangeConversations(file.path, diff);
    if (entry) fileEntries.push(entry);
  }

  return {
    id: `narrative-${commitSha}`,
    version: '0.1.0',
    timestamp: new Date().toISOString(),
    vcs: { type: 'git', revision: commitSha },
    tool: { name: 'narrative', version: '0.1.0' },
    files: fileEntries,
    metadata: {
      'dev.narrative': {
        derived: true,
        sessionId: sessionId ?? null
      }
    }
  };
}

export async function writeGeneratedTraceRecord(repoRoot: string, record: TraceRecord): Promise<string> {
  const fileName = `${record.timestamp.replace(/[:.]/g, '-')}_${record.id}${TRACE_EXTENSION}`;
  const rel = `${TRACE_GENERATED_DIR}/${fileName}`;
  await writeNarrativeFile(repoRoot, rel, JSON.stringify(record, null, 2));
  return rel;
}
