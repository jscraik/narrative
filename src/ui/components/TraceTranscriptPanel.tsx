import { useMemo, useState } from 'react';
import { Bot, Sparkles, User, Terminal, Lightbulb, Wrench, ChevronDown, ChevronUp, Copy, Check } from 'lucide-react';
import type { SessionExcerpt, SessionMessage, SessionMessageRole } from '../../core/types';

const ROLE_CONFIG: Record<SessionMessageRole, {
  label: string;
  badgeClass: string;
  icon: typeof User;
  description: string;
}> = {
  user: {
    label: 'You',
    badgeClass: 'bg-sky-100 text-sky-700 border-sky-200',
    icon: User,
    description: 'User prompt'
  },
  assistant: {
    label: 'Assistant',
    badgeClass: 'bg-stone-100 text-stone-700 border-stone-200',
    icon: Bot,
    description: 'Assistant response'
  },
  thinking: {
    label: 'Thinking',
    badgeClass: 'bg-amber-100 text-amber-700 border-amber-200',
    icon: Lightbulb,
    description: 'Model reasoning process'
  },
  plan: {
    label: 'Plan',
    badgeClass: 'bg-violet-100 text-violet-700 border-violet-200',
    icon: Sparkles,
    description: 'Execution plan'
  },
  tool_call: {
    label: 'Tool',
    badgeClass: 'bg-emerald-100 text-emerald-700 border-emerald-200',
    icon: Wrench,
    description: 'Tool invocation'
  }
};

function messageTitle(message: SessionMessage): string {
  if (message.role === 'tool_call' && message.toolName) {
    return message.toolName;
  }
  if (message.role === 'thinking') {
    return 'Internal reasoning';
  }
  if (message.role === 'plan') {
    return 'Execution plan';
  }
  return ROLE_CONFIG[message.role].label;
}

function roleBadge(role: SessionMessageRole) {
  const config = ROLE_CONFIG[role];
  const Icon = config.icon;

  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-medium border ${config.badgeClass}`}>
      <Icon className="h-3 w-3" />
      {config.label}
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

function ToolCallDetails({ message }: { message: SessionMessage }) {
  const [isExpanded, setIsExpanded] = useState(true);
  const [copied, setCopied] = useState(false);
  const toolInput = formatToolInput(message);
  const hasInput = toolInput && toolInput.length > 0;

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(toolInput);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Ignore copy errors
    }
  };

  return (
    <div className="mt-3 rounded-lg border border-emerald-200 bg-emerald-50/50 overflow-hidden">
      <button
        type="button"
        onClick={() => setIsExpanded(!isExpanded)}
        className="w-full flex items-center justify-between px-3 py-2 text-xs font-medium text-emerald-800 hover:bg-emerald-100/50 transition-colors"
      >
        <span className="flex items-center gap-2">
          <Terminal className="w-3.5 h-3.5" />
          Tool Input
        </span>
        {isExpanded ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
      </button>
      
      {isExpanded && (
        <div className="border-t border-emerald-200">
          <div className="flex items-center justify-between px-3 py-1.5 bg-emerald-100/30 border-b border-emerald-200">
            <span className="text-[10px] uppercase tracking-wide text-emerald-600">
              Arguments
            </span>
            <button
              type="button"
              onClick={handleCopy}
              className="inline-flex items-center gap-1 text-[10px] text-emerald-600 hover:text-emerald-800 transition-colors"
            >
              {copied ? (
                <><Check className="w-3 h-3" /> Copied</>
              ) : (
                <><Copy className="w-3 h-3" /> Copy</>
              )}
            </button>
          </div>
          <div className="p-3">
            {hasInput ? (
              <pre className="text-[11px] text-emerald-900 whitespace-pre-wrap break-words font-mono leading-relaxed">
                {toolInput}
              </pre>
            ) : (
              <span className="text-[11px] text-emerald-600 italic">No input recorded</span>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function ThinkingBlock({ message }: { message: SessionMessage }) {
  const [isExpanded, setIsExpanded] = useState(false);
  const text = message.text ?? '';
  const isLong = text.length > 200;
  const displayText = isExpanded || !isLong ? text : `${text.slice(0, 200)}...`;

  return (
    <div className="mt-2">
      <div className="text-sm text-amber-800 leading-relaxed whitespace-pre-wrap break-words">
        {displayText}
      </div>
      {isLong && (
        <button
          type="button"
          onClick={() => setIsExpanded(!isExpanded)}
          className="mt-2 text-xs text-amber-600 hover:text-amber-800 font-medium flex items-center gap-1 transition-colors"
        >
          {isExpanded ? (
            <><ChevronUp className="w-3.5 h-3.5" /> Show less</>
          ) : (
            <><ChevronDown className="w-3.5 h-3.5" /> Show full reasoning</>
          )}
        </button>
      )}
    </div>
  );
}

function MessageCard({
  message,
  selectedFile,
  onFileClick
}: {
  message: SessionMessage;
  selectedFile?: string | null;
  onFileClick?: (path: string) => void;
}) {
  const config = ROLE_CONFIG[message.role];

  return (
    <div className={`rounded-xl border px-4 py-3 transition-all hover:shadow-sm min-w-0 ${
      message.role === 'user' ? 'bg-sky-50/50 border-sky-200' :
      message.role === 'thinking' ? 'bg-amber-50/50 border-amber-200' :
      message.role === 'plan' ? 'bg-violet-50/50 border-violet-200' :
      message.role === 'tool_call' ? 'bg-emerald-50/50 border-emerald-200' :
      'bg-white border-stone-200'
    }`}>
      {/* Header */}
      <div className="flex items-center justify-between gap-3 min-w-0">
        <div className="flex items-center gap-2 min-w-0">
          {roleBadge(message.role)}
          <span className={`text-xs font-semibold truncate ${config.badgeClass.split(' ')[1]}`}>
            {messageTitle(message)}
          </span>
        </div>
        <span className="text-[10px] text-stone-400">
          {config.description}
        </span>
      </div>

      {/* Content based on role */}
      {message.role === 'thinking' ? (
        <ThinkingBlock message={message} />
      ) : message.role === 'tool_call' ? (
        <ToolCallDetails message={message} />
      ) : (
        <div className="mt-2 text-sm text-stone-700 leading-relaxed whitespace-pre-wrap break-words">
          {message.text || (
            <span className="text-stone-400 italic">No message content</span>
          )}
        </div>
      )}

      {/* File pills */}
      {message.files && message.files.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
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
      )}
    </div>
  );
}

function EmptyState() {
  return (
    <div className="card p-5 overflow-x-hidden">
      <div className="flex items-center justify-between">
        <div>
          <div className="section-header">CONVERSATION</div>
          <div className="section-subheader mt-0.5">Session context</div>
        </div>
      </div>
      <div className="mt-6 flex flex-col items-center text-center py-4">
        <div className="w-12 h-12 rounded-full bg-stone-100 flex items-center justify-center mb-3">
          <Sparkles className="w-5 h-5 text-stone-400" />
        </div>
        <p className="text-sm text-stone-500 mb-1">No conversation loaded</p>
        <p className="text-xs text-stone-400 max-w-[280px]">
          Import a session to see the full conversation including thinking, planning, and tool calls.
        </p>
      </div>
    </div>
  );
}

function StatsBar({ stats }: { stats: Record<SessionMessageRole, number> }) {
  const items = [
    { count: stats.user, label: 'user', color: 'text-sky-600' },
    { count: stats.assistant, label: 'assistant', color: 'text-stone-600' },
    { count: stats.thinking, label: 'thinking', color: 'text-amber-600' },
    { count: stats.plan, label: 'plan', color: 'text-violet-600' },
    { count: stats.tool_call, label: 'tools', color: 'text-emerald-600' },
  ].filter(item => item.count > 0);

  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px]">
      {items.map(({ count, label, color }) => (
        <span key={label} className="flex items-center gap-1">
          <span className={`font-semibold ${color}`}>{count}</span>
          <span className="text-stone-400">{label}</span>
        </span>
      ))}
    </div>
  );
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

  const messages = excerpt?.messages ?? [];
  const stats = useMemo(() => roleSummary(messages), [messages]);
  const visibleMessages = showAll ? messages : messages.slice(0, 8);
  const hiddenCount = messages.length - visibleMessages.length;

  if (!excerpt || messages.length === 0) {
    return <EmptyState />;
  }

  return (
    <div className="card p-5">
      {/* Header */}
      <div className="flex items-start justify-between gap-4 mb-4">
        <div>
          <div className="section-header">CONVERSATION</div>
          <div className="section-subheader mt-0.5">
            {excerpt.tool} · {messages.length} messages
          </div>
        </div>
        <StatsBar stats={stats} />
      </div>

      {/* Messages */}
      <div className="space-y-3">
        {visibleMessages.map((message) => (
          <MessageCard
            key={message.id}
            message={message}
            selectedFile={selectedFile}
            onFileClick={onFileClick}
          />
        ))}
      </div>

      {/* Show more/less */}
      {hiddenCount > 0 && (
        <div className="mt-4 flex justify-center">
          <button
            type="button"
            onClick={() => setShowAll(!showAll)}
            className="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-stone-100 text-stone-600 text-xs font-medium hover:bg-stone-200 transition-colors"
          >
            {showAll ? (
              <><ChevronUp className="w-3.5 h-3.5" /> Show less</>
            ) : (
              <><ChevronDown className="w-3.5 h-3.5" /> Show {hiddenCount} more messages</>
            )}
          </button>
        </div>
      )}
    </div>
  );
}
