import type { CommitDetails, CommitSummary, FileChange } from '../types';
import { execProgram, ShellError } from './shell';

function assertOk(result: { code: number | null; stdout: string; stderr: string }, context: string) {
  if (result.code !== 0 && result.code !== null) {
    throw new ShellError(`${context} (exit ${result.code})`, result);
  }
}

export async function git(cwd: string, args: string[]) {
  const out = await execProgram('git', args, { cwd });
  assertOk(out, `git ${args.join(' ')}`);
  return out.stdout;
}

export async function resolveGitRoot(path: string): Promise<string> {
  const stdout = await git(path, ['rev-parse', '--show-toplevel']);
  return stdout.trim();
}

export async function getHeadBranch(path: string): Promise<string> {
  const stdout = await git(path, ['rev-parse', '--abbrev-ref', 'HEAD']);
  return stdout.trim();
}

export async function getHeadSha(path: string): Promise<string> {
  const stdout = await git(path, ['rev-parse', 'HEAD']);
  return stdout.trim();
}

export async function listCommits(path: string, limit = 50): Promise<CommitSummary[]> {
  const format = '%H%x1f%an%x1f%aI%x1f%s%x1e'; // Use %aI for strict ISO 8601 format
  const stdout = await git(path, [
    'log',
    '-n',
    String(limit),
    `--pretty=format:${format}`,
    '--no-color'
  ]);

  const records = stdout.split('\x1e').map((s) => s.trim()).filter(Boolean);
  const commits: CommitSummary[] = [];

  for (const rec of records) {
    const [sha, author, authoredAtISO, subject] = rec.split('\x1f');
    if (!sha) continue;
    commits.push({
      sha,
      author: author ?? 'unknown',
      authoredAtISO: authoredAtISO ?? new Date().toISOString(),
      subject: subject ?? ''
    });
  }

  return commits;
}

export function parseNumstat(output: string): FileChange[] {
  // numstat lines look like: "<additions>\t<deletions>\t<path>"
  // additions/deletions can be '-' for binary files.
  const changes: FileChange[] = [];
  const lines = output.split(/\r?\n/);

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const parts = trimmed.split('\t');
    if (parts.length < 3) continue;
    const [aRaw, dRaw, path] = parts;
    const additions = aRaw === '-' ? 0 : Number.parseInt(aRaw ?? '0', 10) || 0;
    const deletions = dRaw === '-' ? 0 : Number.parseInt(dRaw ?? '0', 10) || 0;
    changes.push({ path, additions, deletions });
  }

  return changes;
}

export async function getCommitDetails(path: string, sha: string): Promise<CommitDetails> {
  const stdout = await git(path, ['show', '--numstat', '--format=', '--no-color', sha]);
  return {
    sha,
    fileChanges: parseNumstat(stdout)
  };
}

export async function getCommitDiffForFile(path: string, sha: string, filePath: string): Promise<string> {
  // Patch only, no commit header
  const stdout = await git(path, ['show', '--format=', '--no-color', sha, '--', filePath]);
  return stdout;
}

export async function getAggregateStatsForCommits(path: string, limit = 50): Promise<{
  added: number;
  removed: number;
  uniqueFiles: number;
}> {
  const stdout = await git(path, ['log', '-n', String(limit), '--numstat', '--pretty=tformat:', '--no-color']);

  let added = 0;
  let removed = 0;
  const fileSet = new Set<string>();

  for (const change of parseNumstat(stdout)) {
    added += change.additions;
    removed += change.deletions;
    fileSet.add(change.path);
  }

  return { added, removed, uniqueFiles: fileSet.size };
}
