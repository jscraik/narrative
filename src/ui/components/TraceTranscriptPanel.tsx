import { useMemo, useState } from 'react';
import { Bot, Sparkles, User } from 'lucide-react';
import type { SessionExcerpt, SessionMessage, SessionMessageRole } from '../../core/types';

const ROLE_LABELS: Record<SessionMessageRole, string> = {
  user: 'User',
  assistant: 'Assistant',
  thinking: 'Thinking',
  plan: 'Plan',
  tool_call: 'Tool'
};

const ROLE_STYLES: Record<SessionMessageRole, { badge: string; text: string }> = {
  user: { badge: 'bg-sky-100 text-sky-700', text: 'text-sky-700' },
  assistant: { badge: 'bg-stone-100 text-stone-600', text: 'text-stone-700' },
  thinking: { badge: 'bg-amber-100 text-amber-700', text: 'text-amber-700' },
  plan: { badge: 'bg-violet-100 text-violet-700', text: 'text-violet-700' },
  tool_call: { badge: 'bg-emerald-100 text-emerald-700', text: 'text-emerald-700' }
};

const ROLE_ICONS: Partial<Record<SessionMessageRole, typeof User>> = {
  user: User,
  assistant: Bot,
  thinking: Sparkles,
  plan: Sparkles,
  tool_call: Bot
};

function messageTitle(message: SessionMessage) {
  if (message.role === 'tool_call' && message.toolName) {
    return `Tool call · ${message.toolName}`;
  }
  return ROLE_LABELS[message.role] ?? 'Message';
}

function roleBadge(role: SessionMessageRole) {
  const label = ROLE_LABELS[role] ?? role;
  const style = ROLE_STYLES[role] ?? ROLE_STYLES.assistant;
  const Icon = ROLE_ICONS[role];

  return (
    <span className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium ${style.badge}`}>
      {Icon ? <Icon className="h-3 w-3" /> : null}
      {label}
    </span>
  );
}

function roleSummary(messages: SessionMessage[]) {
  return messages.reduce<Record<SessionMessageRole, number>>((acc, message) => {
    acc[message.role] = (acc[message.role] ?? 0) + 1;
    return acc;
  }, {
    user: 0,
    assistant: 0,
    thinking: 0,
    plan: 0,
    tool_call: 0
  });
}

function formatToolInput(message: SessionMessage): string {
  if (message.toolInput !== undefined) {
    if (typeof message.toolInput === 'string') {
      return message.toolInput;
    }
    try {
      return JSON.stringify(message.toolInput, null, 2);
    } catch {
      return String(message.toolInput);
    }
  }
  return message.text ?? '';
}

export function TraceTranscriptPanel({
  excerpt,
  selectedFile,
  onFileClick
}: {
  excerpt?: SessionExcerpt;
  selectedFile?: string | null;
  onFileClick?: (path: string) => void;
}) {
  const [showAll, setShowAll] = useState(false);
  const [expandedMessages, setExpandedMessages] = useState<Set<string>>(() => new Set());
  const [copiedMessageId, setCopiedMessageId] = useState<string | null>(null);

  const messages = excerpt?.messages ?? [];
  const stats = useMemo(() => roleSummary(messages), [messages]);
  const visibleMessages = showAll ? messages : messages.slice(0, 8);
  const hiddenCount = messages.length - visibleMessages.length;

  if (!excerpt || messages.length === 0) {
    return (
      <div className="card p-5">
        <div className="flex items-center justify-between">
          <div>
            <div className="section-header">TRACE TRANSCRIPT</div>
            <div className="section-subheader mt-0.5">session context</div>
          </div>
        </div>
        <div className="mt-6 flex flex-col items-center text-center py-4">
          <div className="w-12 h-12 rounded-full bg-stone-100 flex items-center justify-center mb-3">
            <Sparkles className="w-5 h-5 text-stone-400" />
          </div>
          <p className="text-sm text-stone-500 mb-1">No transcript loaded</p>
          <p className="text-xs text-stone-400">Import a session to surface plan, thinking, and tool calls.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="card p-5">
      <div className="flex items-start justify-between gap-4">
        <div>
          <div className="section-header">TRACE TRANSCRIPT</div>
          <div className="section-subheader mt-0.5">session context</div>
          <div className="mt-2 text-xs text-stone-500">
            {excerpt.tool} · {messages.length} messages
          </div>
        </div>
        <div className="flex flex-wrap gap-1.5 text-[10px] text-stone-400">
          {stats.user > 0 ? <span>{stats.user} user</span> : null}
          {stats.assistant > 0 ? <span>{stats.assistant} assistant</span> : null}
          {stats.thinking > 0 ? <span>{stats.thinking} thinking</span> : null}
          {stats.plan > 0 ? <span>{stats.plan} plan</span> : null}
          {stats.tool_call > 0 ? <span>{stats.tool_call} tools</span> : null}
        </div>
      </div>

      <div className="mt-4 space-y-3">
        {visibleMessages.map((message) => {
          const style = ROLE_STYLES[message.role] ?? ROLE_STYLES.assistant;
          const hasToolInput =
            message.role === 'tool_call' && (message.toolInput !== undefined || message.text);
          const isExpanded = expandedMessages.has(message.id);
          const toolInput = hasToolInput ? formatToolInput(message) : '';
          const displayText =
            message.role === 'tool_call' && hasToolInput ? 'Tool input captured.' : (message.text ?? 'Transcript entry captured.');
          return (
            <div key={message.id} className="rounded-lg border border-stone-100 bg-white px-3 py-2">
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  {roleBadge(message.role)}
                  <span className={`text-xs font-medium ${style.text}`}>{messageTitle(message)}</span>
                </div>
              </div>
              <div className="mt-2 text-sm text-stone-600 whitespace-pre-wrap break-words">
                {displayText}
              </div>
              {hasToolInput ? (
                <div className="mt-2">
                  <button
                    type="button"
                    onClick={() => {
                      setExpandedMessages((prev) => {
                        const next = new Set(prev);
                        if (next.has(message.id)) {
                          next.delete(message.id);
                        } else {
                          next.add(message.id);
                        }
                        return next;
                      });
                    }}
                    className="text-[11px] font-medium text-sky-600 hover:text-sky-700"
                    aria-expanded={isExpanded}
                  >
                    {isExpanded ? 'Hide input' : 'Show input'}
                  </button>
                  {isExpanded ? (
                    <div className="mt-2 rounded-md border border-stone-200 bg-stone-50 p-2">
                      <div className="flex items-center justify-between gap-2">
                        <span className="text-[10px] uppercase tracking-wide text-stone-400">Tool input</span>
                        <button
                          type="button"
                          onClick={async () => {
                            try {
                              await navigator.clipboard.writeText(toolInput || '');
                              setCopiedMessageId(message.id);
                              window.setTimeout(() => setCopiedMessageId(null), 1200);
                            } catch {
                              setCopiedMessageId(null);
                            }
                          }}
                          className="text-[10px] font-medium text-stone-500 hover:text-stone-700"
                        >
                          {copiedMessageId === message.id ? 'Copied' : 'Copy'}
                        </button>
                      </div>
                      <pre className="mt-2 text-[11px] text-stone-600 whitespace-pre-wrap break-words">
                        {toolInput || 'No input recorded.'}
                      </pre>
                    </div>
                  ) : null}
                </div>
              ) : null}
              {message.files && message.files.length > 0 ? (
                <div className="mt-2 flex flex-wrap gap-2">
                  {message.files.map((file) => (
                    <button
                      key={file}
                      type="button"
                      onClick={() => onFileClick?.(file)}
                      aria-pressed={selectedFile === file}
                      title={file}
                      className={`pill-file max-w-full truncate ${selectedFile === file ? 'selected' : ''}`}
                    >
                      {file}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          );
        })}
      </div>

      {hiddenCount > 0 ? (
        <div className="mt-3 flex justify-end">
          <button
            type="button"
            onClick={() => setShowAll(!showAll)}
            className="text-xs font-medium text-sky-600 hover:text-sky-700"
          >
            {showAll ? 'Show less' : `Show ${hiddenCount} more`}
          </button>
        </div>
      ) : null}
    </div>
  );
}
