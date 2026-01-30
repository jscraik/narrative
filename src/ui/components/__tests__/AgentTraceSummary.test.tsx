import { render, screen } from '@testing-library/react';
import { AgentTraceSummary } from '../AgentTraceSummary';

describe('AgentTraceSummary', () => {
  it('renders empty state when no summary is provided', () => {
    render(<AgentTraceSummary hasFiles={false} />);

    expect(screen.getByText('AI ATTRIBUTION')).toBeInTheDocument();
    expect(screen.getByText(/No Agent Trace yet/)).toBeInTheDocument();
  });

  it('shows active status when provided', () => {
    render(
      <AgentTraceSummary
        hasFiles={true}
        status={{ state: 'active', message: 'Running', lastSeenAtISO: new Date().toISOString() }}
      />
    );

    expect(screen.getByText(/Codex OTel: Active/)).toBeInTheDocument();
    expect(screen.getByText(/Running/)).toBeInTheDocument();
  });
});
