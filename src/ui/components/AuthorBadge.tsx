import { Bot, User, Users, HelpCircle } from 'lucide-react';
import { formatToolName } from '../../core/attribution-api';

export interface SourceLine {
  lineNumber: number;
  content: string;
  authorType: 'human' | 'ai_agent' | 'ai_tab' | 'mixed' | 'unknown';
  sessionId?: string;
  aiPercentage?: number;
  tool?: string;
  model?: string;
  traceAvailable?: boolean;
}

function getBadgeLabel(line: SourceLine): string {
  if (line.authorType === 'ai_agent') {
    return line.tool ? formatToolName(line.tool) : 'Agent';
  }
  if (line.authorType === 'ai_tab') {
    return line.tool ? `${formatToolName(line.tool)} Assist` : 'Assist';
  }
  if (line.authorType === 'mixed') {
    return `Mixed ${line.aiPercentage ?? 50}%`;
  }
  if (line.authorType === 'human') {
    return 'Human';
  }
  return 'Unknown';
}

function getBadgeTitle(line: SourceLine): string {
  const parts: string[] = [];
  if (line.tool) parts.push(`Tool: ${formatToolName(line.tool)}`);
  if (line.model) parts.push(`Model: ${line.model}`);
  if (line.sessionId) parts.push(`Session: ${line.sessionId}`);
  if (line.traceAvailable === false) parts.push('Trace: local-only (not available)');
  return parts.join(' · ');
}

function formatBadgeTitle(detail: string, meta: string): string {
  if (!meta) return detail;
  return `${detail} · ${meta}`;
}

export interface AuthorBadgeProps {
  line: SourceLine;
}

export function AuthorBadge({ line }: AuthorBadgeProps) {
  const label = getBadgeLabel(line);
  const title = getBadgeTitle(line);

  switch (line.authorType) {
    case 'ai_agent':
      return (
        <span
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-emerald-100 text-emerald-700"
          title={formatBadgeTitle('AI-generated', title)}
        >
          <Bot className="w-3 h-3" />
          {label}
        </span>
      );
    case 'ai_tab':
      return (
        <span
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-blue-100 text-blue-700"
          title={formatBadgeTitle('Assist suggestions', title)}
        >
          <Bot className="w-3 h-3" />
          {label}
        </span>
      );
    case 'mixed':
      return (
        <span
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-amber-100 text-amber-700"
          title={formatBadgeTitle('Modified lines (AI + human edits)', title)}
        >
          <Users className="w-3 h-3" />
          {label}
        </span>
      );
    case 'human':
      return (
        <span
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-stone-100 text-stone-600"
          title={formatBadgeTitle('Human-authored', title)}
        >
          <User className="w-3 h-3" />
          {label}
        </span>
      );
    default:
      return (
        <span
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium bg-stone-100 text-stone-400"
          title={formatBadgeTitle('Unknown source', title)}
        >
          <HelpCircle className="w-3 h-3" />
          {label}
        </span>
      );
  }
}

export function getLineColor(authorType: string): string {
  switch (authorType) {
    case 'ai_agent':
      return 'bg-emerald-50/30 hover:bg-emerald-50/50';
    case 'ai_tab':
      return 'bg-blue-50/30 hover:bg-blue-50/50';
    case 'mixed':
      return 'bg-amber-50/30 hover:bg-amber-50/50';
    case 'human':
      return '';
    default:
      return '';
  }
}
