import Database from '@tauri-apps/plugin-sql';
import type { CommitDetails, CommitSummary, FileChange } from '../types';

const yieldToMain = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

let _db: Database | null = null;

export async function getDb(): Promise<Database> {
  if (_db) return _db;
  _db = await Database.load('sqlite:narrative.db');
  return _db;
}

export async function upsertRepo(path: string): Promise<number> {
  const db = await getDb();
  await db.execute('INSERT OR IGNORE INTO repos (path) VALUES ($1)', [path]);
  await db.execute('UPDATE repos SET last_opened_at = strftime(\'%Y-%m-%dT%H:%M:%fZ\',\'now\') WHERE path = $1', [
    path
  ]);

  const rows = await db.select<{ id: number }[]>('SELECT id FROM repos WHERE path = $1', [path]);
  if (!rows?.[0]?.id) throw new Error('failed to resolve repo id');
  return rows[0].id;
}

export async function cacheCommitSummaries(
  repoId: number,
  commits: CommitSummary[],
  onProgress?: (current: number, total: number) => void
): Promise<void> {
  const db = await getDb();
  // Insert in a transaction-ish batch (sqlite will autocommit per statement; that's OK for MVP)
  for (let index = 0; index < commits.length; index += 1) {
    const c = commits[index];
    await db.execute(
      'INSERT OR IGNORE INTO commits (repo_id, sha, author, authored_at, subject, body) VALUES ($1, $2, $3, $4, $5, $6)',
      [repoId, c.sha, c.author, c.authoredAtISO, c.subject, '']
    );
    const current = index + 1;
    if (onProgress && (current % 25 === 0 || current === commits.length)) {
      onProgress(current, commits.length);
    }
    if (current % 50 === 0) {
      await yieldToMain();
    }
  }
}

export async function getCachedFileChanges(repoId: number, sha: string): Promise<FileChange[] | null> {
  const db = await getDb();
  const rows = await db.select<{ path: string; additions: number; deletions: number }[]>(
    'SELECT path, additions, deletions FROM file_changes WHERE repo_id = $1 AND commit_sha = $2 ORDER BY additions + deletions DESC',
    [repoId, sha]
  );
  if (!rows || rows.length === 0) return null;
  return rows.map((r) => ({ path: r.path, additions: r.additions, deletions: r.deletions }));
}

export async function cacheFileChanges(repoId: number, details: CommitDetails): Promise<void> {
  const db = await getDb();
  for (const fc of details.fileChanges) {
    await db.execute(
      'INSERT OR REPLACE INTO file_changes (repo_id, commit_sha, path, additions, deletions) VALUES ($1, $2, $3, $4, $5)',
      [repoId, details.sha, fc.path, fc.additions, fc.deletions]
    );
  }
}
