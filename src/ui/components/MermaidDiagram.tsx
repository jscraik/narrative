import { useEffect, useState } from 'react';
import { renderMermaid } from 'beautiful-mermaid';

interface MermaidDiagramProps {
  chart: string;
}

/**
 * Renders a Mermaid diagram using beautiful-mermaid.
 * Creates beautiful, themeable SVG diagrams.
 */
export function MermaidDiagram({ chart }: MermaidDiagramProps) {
  const [svg, setSvg] = useState<string>('');
  const [error, setError] = useState<string>('');

  useEffect(() => {
    const renderDiagram = async () => {
      if (!chart) return;

      try {
        const renderedSvg = await renderMermaid(chart);
        setSvg(renderedSvg);
        setError('');
      } catch (err) {
        console.error('Mermaid render error:', err);
        setError('Failed to render diagram');
        setSvg('');
      }
    };

    renderDiagram();
  }, [chart]);

  if (error) {
    return (
      <div className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700">
        <p className="font-medium">Diagram Error</p>
        <p className="mt-1 text-red-600">{error}</p>
        <pre className="mt-2 rounded bg-red-100 p-2 text-xs overflow-auto">{chart}</pre>
      </div>
    );
  }

  if (!svg) {
    return (
      <div className="my-4 rounded-lg border border-stone-200 bg-stone-50 p-8 flex items-center justify-center">
        <div className="text-sm text-stone-500">Rendering diagram...</div>
      </div>
    );
  }

  return (
    <div 
      className="mermaid-diagram my-4 rounded-lg border border-stone-200 bg-white p-4 overflow-auto"
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
