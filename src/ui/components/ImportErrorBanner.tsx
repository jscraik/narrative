import { AlertCircle, FileJson, HelpCircle, X } from 'lucide-react';

interface ImportErrorBannerProps {
  error: string;
  onDismiss?: () => void;
}

/**
 * Maps error messages to helpful documentation and recovery actions.
 * This provides context-aware help for common import failures.
 */
function getErrorHelp(error: string): {
  title: string;
  description: string;
  actions: Array<{ label: string; href?: string; action?: () => void }>;
} | null {
  const lowerError = error.toLowerCase();

  if (lowerError.includes('no readable messages found in the kimi context log')) {
    return {
      title: 'Kimi Log Format Issue',
      description: 'The file was read successfully, but we could not find any messages in the expected format. Kimi logs should be JSON Lines (.jsonl) with records containing role and content fields.',
      actions: [
        {
          label: 'View Kimi Log Format Guide',
          href: 'https://docs.narrative.dev/kimi-log-format'
        },
        {
          label: 'Try importing as Generic JSON',
          action: () => {
            // This would trigger the generic JSON import flow
            // For now, we just log - the user can manually retry
            console.log('User chose to retry as generic JSON');
          }
        }
      ]
    };
  }

  if (lowerError.includes('json') && lowerError.includes('parse')) {
    return {
      title: 'Invalid JSON Format',
      description: 'The file could not be parsed as JSON. Please ensure the file contains valid JSON with proper syntax.',
      actions: [
        {
          label: 'Validate JSON Online',
          href: 'https://jsonlint.com/'
        }
      ]
    };
  }

  if (lowerError.includes('file') || lowerError.includes('not found')) {
    return {
      title: 'File Access Error',
      description: 'Could not read the selected file. Please ensure the file exists and you have permission to access it.',
      actions: []
    };
  }

  return null;
}

export function ImportErrorBanner({ error, onDismiss }: ImportErrorBannerProps) {
  const help = getErrorHelp(error);

  return (
    <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm">
      <div className="flex items-start gap-3">
        <AlertCircle className="w-5 h-5 text-red-500 mt-0.5 shrink-0" />
        <div className="flex-1 min-w-0">
          {help ? (
            <>
              <div className="font-semibold text-red-800 mb-1">{help.title}</div>
              <div className="text-red-700 mb-3 leading-relaxed">{help.description}</div>
              
              {help.actions.length > 0 && (
                <div className="flex flex-wrap gap-2">
                  {help.actions.map((action, idx) => (
                    action.href ? (
                      <a
                        key={idx}
                        href={action.href}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-white border border-red-200 text-red-700 text-xs font-medium hover:bg-red-100 transition-colors"
                      >
                        <HelpCircle className="w-3.5 h-3.5" />
                        {action.label}
                      </a>
                    ) : (
                      <button
                        key={idx}
                        type="button"
                        onClick={action.action}
                        className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-white border border-red-200 text-red-700 text-xs font-medium hover:bg-red-100 transition-colors"
                      >
                        <FileJson className="w-3.5 h-3.5" />
                        {action.label}
                      </button>
                    )
                  ))}
                </div>
              )}
            </>
          ) : (
            <div className="text-red-700">{error}</div>
          )}
        </div>
        {onDismiss && (
          <button
            type="button"
            onClick={onDismiss}
            className="text-red-400 hover:text-red-600 transition-colors shrink-0"
            aria-label="Dismiss error"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>
    </div>
  );
}
