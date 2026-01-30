import { useEffect, useRef, useState } from 'react';
import { renderMermaid } from 'beautiful-mermaid';

interface MermaidDiagramProps {
  chart: string;
}

/**
 * Renders a Mermaid diagram using beautiful-mermaid.
 * Creates beautiful, themeable SVG diagrams.
 */
export function MermaidDiagram({ chart }: MermaidDiagramProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string>('');

  useEffect(() => {
    const renderDiagram = async () => {
      if (!chart || !containerRef.current) return;

      try {
        const renderedSvg = await renderMermaid(chart);
        // Safely inject SVG into the container using DOM manipulation
        // This is safe because renderMermaid returns trusted SVG content
        containerRef.current.innerHTML = renderedSvg;
        setError('');
      } catch (err) {
        console.error('Mermaid render error:', err);
        setError('Failed to render diagram');
        if (containerRef.current) {
          containerRef.current.innerHTML = '';
        }
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

  return (
    <div 
      ref={containerRef}
      className="mermaid-diagram my-4 rounded-lg border border-stone-200 bg-white p-4 overflow-auto"
    />
  );
}
