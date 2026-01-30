import { useState, useCallback } from 'react';
import { Upload, AlertTriangle, CheckCircle, XCircle, RefreshCw } from 'lucide-react';
import { 
  scanForSessionFiles, 
  importSessionFile, 
  importSessionFiles,
  type ScannedSession,
  type BatchImportResult 
} from '../../core/attribution-api';

interface SessionImportPanelProps {
  repoId: number;
}

export function SessionImportPanel({ repoId }: SessionImportPanelProps) {
  const [sessions, setSessions] = useState<ScannedSession[]>([]);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [importing, setImporting] = useState(false);
  const [result, setResult] = useState<BatchImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);

  const handleScan = useCallback(async () => {
    setScanning(true);
    setError(null);
    try {
      const found = await scanForSessionFiles();
      setSessions(found);
      if (found.length === 0) {
        setError('No session files found in standard locations (~/.claude, ~/.cursor, etc.)');
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Scan failed');
    } finally {
      setScanning(false);
    }
  }, []);

  const handleImportSelected = useCallback(async () => {
    if (selectedPaths.size === 0) return;
    
    setImporting(true);
    setError(null);
    setResult(null);
    
    try {
      const result = await importSessionFiles(repoId, Array.from(selectedPaths));
      setResult(result);
      // Clear selection after successful import
      if (result.failed.length === 0) {
        setSelectedPaths(new Set());
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Import failed');
    } finally {
      setImporting(false);
    }
  }, [repoId, selectedPaths]);

  const handleImportSingle = useCallback(async (path: string) => {
    setImporting(true);
    setError(null);
    setResult(null);
    
    try {
      const result = await importSessionFile(repoId, path);
      setResult(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Import failed');
    } finally {
      setImporting(false);
    }
  }, [repoId]);

  const toggleSelection = useCallback((path: string) => {
    setSelectedPaths(prev => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }, []);

  return (
    <div className="bg-white rounded-lg border border-stone-200 p-4">
      <div className="flex items-center justify-between mb-4">
        <h3 className="font-medium text-stone-800">Import AI Sessions</h3>
        <button
          type="button"
          onClick={handleScan}
          disabled={scanning}
          className="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-stone-600 bg-stone-50 border border-stone-200 rounded-md hover:bg-stone-100 disabled:opacity-50 transition-colors"
        >
          <RefreshCw className={`w-4 h-4 ${scanning ? 'animate-spin' : ''}`} />
          {scanning ? 'Scanning...' : 'Scan for Sessions'}
        </button>
      </div>

      {error && (
        <div className="mb-4 p-3 bg-red-50 border border-red-200 rounded-md flex items-start gap-2">
          <AlertTriangle className="w-4 h-4 text-red-500 mt-0.5 flex-shrink-0" />
          <p className="text-sm text-red-700">{error}</p>
        </div>
      )}

      {sessions.length > 0 && (
        <div className="mb-4">
          <div className="flex items-center justify-between mb-2">
            <p className="text-sm text-stone-600">
              Found {sessions.length} session file{sessions.length !== 1 ? 's' : ''}
            </p>
            {selectedPaths.size > 0 && (
              <button
                type="button"
                onClick={handleImportSelected}
                disabled={importing}
                className="flex items-center gap-2 px-3 py-1.5 text-sm font-medium text-white bg-sky-600 rounded-md hover:bg-sky-700 disabled:opacity-50 transition-colors"
              >
                <Upload className="w-4 h-4" />
                {importing ? 'Importing...' : `Import ${selectedPaths.size} Selected`}
              </button>
            )}
          </div>

          <div className="border border-stone-200 rounded-md divide-y divide-stone-200 max-h-64 overflow-auto">
            {sessions.map((session) => {
              const isSelected = selectedPaths.has(session.path);
              const fileName = session.path.split('/').pop() || session.path;
              const checkboxId = `session-${session.path.replace(/[^a-zA-Z0-9_-]/g, '-')}`;
              
              return (
                <div
                  key={session.path}
                  className={`flex items-center justify-between p-3 hover:bg-stone-50 transition-colors ${
                    isSelected ? 'bg-sky-50' : ''
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <input
                      id={checkboxId}
                      type="checkbox"
                      checked={isSelected}
                      onChange={() => toggleSelection(session.path)}
                      className="w-4 h-4 text-sky-600 border-stone-300 rounded focus:ring-sky-500"
                    />
                    <label htmlFor={checkboxId} className="cursor-pointer">
                      <p className="text-sm font-medium text-stone-800">{fileName}</p>
                      <p className="text-xs text-stone-500">{session.tool}</p>
                    </label>
                  </div>
                  <button
                    type="button"
                    onClick={() => handleImportSingle(session.path)}
                    disabled={importing}
                    className="text-xs font-medium text-sky-600 hover:text-sky-700 disabled:opacity-50"
                  >
                    Import
                  </button>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {result && (
        <div className={`p-3 rounded-md border ${
          result.failed.length === 0 
            ? 'bg-emerald-50 border-emerald-200' 
            : 'bg-amber-50 border-amber-200'
        }`}>
          <div className="flex items-center gap-2 mb-2">
            {result.failed.length === 0 ? (
              <>
                <CheckCircle className="w-4 h-4 text-emerald-500" />
                <p className="text-sm font-medium text-emerald-800">
                  Successfully imported {result.succeeded.length} session{result.succeeded.length !== 1 ? 's' : ''}
                </p>
              </>
            ) : (
              <>
                <AlertTriangle className="w-4 h-4 text-amber-500" />
                <p className="text-sm font-medium text-amber-800">
                  Imported {result.succeeded.length} of {result.total} sessions
                </p>
              </>
            )}
          </div>

          {result.succeeded.length > 0 && result.succeeded.some(s => s.warnings.length > 0) && (
            <div className="mt-2 space-y-1">
              {result.succeeded
                .filter(s => s.warnings.length > 0)
                .map((s) => (
                  <p key={s.path} className="text-xs text-amber-700">
                    {s.path.split('/').pop()}: {s.warnings.length} warning{s.warnings.length !== 1 ? 's' : ''}
                  </p>
                ))}
            </div>
          )}

          {result.failed.length > 0 && (
            <div className="mt-2 space-y-1">
              {result.failed.map((f) => (
                <div key={f.path} className="flex items-start gap-1.5">
                  <XCircle className="w-3 h-3 text-red-500 mt-0.5 flex-shrink-0" />
                  <div>
                    <p className="text-xs font-medium text-red-700">
                      {f.path.split('/').pop()}
                    </p>
                    <p className="text-xs text-red-600">{f.error}</p>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="mt-4 p-3 bg-stone-50 rounded-md">
        <p className="text-xs text-stone-600">
          <strong>Supported locations:</strong> ~/.claude/projects/, ~/.cursor/composer/, ~/.continue/
        </p>
        <p className="text-xs text-stone-500 mt-1">
          Sessions are scanned for secrets before import. Files with potential secrets will be flagged.
        </p>
      </div>
    </div>
  );
}
