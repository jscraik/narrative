import { AiContributionBadge } from './AiContributionBadge';
import type { TimelineBadge } from '../../core/types';

export interface BadgePillProps {
  badge: TimelineBadge;
}

export function BadgePill({ badge }: BadgePillProps) {
  if (badge.type === 'test') {
    if (badge.status === 'failed') {
      return (
        <span className="pill-test-failed">
          <span className="text-red-500">✕</span>
          {badge.label}
        </span>
      );
    }
    if (badge.status === 'passed') {
      return (
        <span className="pill-test-passed">
          <span className="text-emerald-500">✓</span>
          {badge.label}
        </span>
      );
    }
  }

  if (badge.type === 'trace') {
    return <span className="pill-trace-ai">{badge.label}</span>;
  }

  if (badge.type === 'contribution' && badge.stats) {
    return (
      <AiContributionBadge
        stats={{
          aiPercentage: badge.stats.aiPercentage,
          primaryTool: badge.stats.tool,
          model: badge.stats.model,
          humanLines: 0,
          aiAgentLines: 0,
          aiAssistLines: 0,
          collaborativeLines: 0,
          totalLines: 0,
        }}
      />
    );
  }

  return <span className="pill-file">{badge.label}</span>;
}
