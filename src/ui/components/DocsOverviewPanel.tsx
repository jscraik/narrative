import { useEffect, useState } from 'react';
import { FileText, BookOpen, X, ChevronRight } from 'lucide-react';
import ReactMarkdown from 'react-markdown';
import rehypeRaw from 'rehype-raw';
import { MermaidDiagram } from './MermaidDiagram';

interface DocFile {
  name: string;
  path: string;
  title: string;
}

interface DocsOverviewPanelProps {
  repoRoot: string;
  onClose?: () => void;
}

/**
 * Component to render markdown documentation files from .narrative/
 * with Mermaid diagram support.
 */
export function DocsOverviewPanel({ repoRoot, onClose }: DocsOverviewPanelProps) {
  const [docs, setDocs] = useState<DocFile[]>([]);
  const [selectedDoc, setSelectedDoc] = useState<DocFile | null>(null);
  const [content, setContent] = useState<string>('');
  const [loading, setLoading] = useState(false);

  // List available documentation files
  useEffect(() => {
    const listDocs = async () => {
      try {
        // In a real implementation, this would call a Tauri command
        // For now, we'll use the known files
        const knownDocs: DocFile[] = [
          {
            name: 'CONVERSATIONS-COMMITS-CODE-MAP.md',
            path: 'CONVERSATIONS-COMMITS-CODE-MAP.md',
            title: 'Architecture Overview',
          },
          {
            name: 'NARRATIVE-ATTRIBUTION-MAP.md',
            path: 'NARRATIVE-ATTRIBUTION-MAP.md',
            title: 'Attribution System',
          },
        ];
        setDocs(knownDocs);
      } catch (error) {
        console.error('Failed to list docs:', error);
      }
    };

    listDocs();
  }, [repoRoot]);

  // Load selected document
  useEffect(() => {
    if (!selectedDoc) {
      setContent('');
      return;
    }

    const loadDoc = async () => {
      setLoading(true);
      try {
        // This would call read_narrative_file in real implementation
        // For now, we'll show a placeholder
        const response = await fetch(`/.narrative/${selectedDoc.path}`);
        if (response.ok) {
          const text = await response.text();
          setContent(text);
        } else {
          setContent(`# ${selectedDoc.title}\n\nThis documentation file provides an overview of the Narrative system architecture.`);
        }
      } catch (error) {
        setContent(`# ${selectedDoc.title}\n\nFailed to load document.\n\nError: ${error}`);
      } finally {
        setLoading(false);
      }
    };

    loadDoc();
  }, [selectedDoc]);

  // Custom components for ReactMarkdown
  const components = {
    code({ node, inline, className, children, ...props }: any) {
      const match = /language-(\w+)/.exec(className || '');
      const language = match?.[1] || '';
      
      if (language === 'mermaid') {
        return <MermaidDiagram chart={String(children).replace(/\n$/, '')} />;
      }
      
      return (
        <code className={className} {...props}>
          {children}
        </code>
      );
    },
  };

  if (selectedDoc) {
    return (
      <div className="card flex flex-col h-full">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-stone-200">
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={() => setSelectedDoc(null)}
              className="p-1.5 rounded-lg hover:bg-stone-100 text-stone-500"
            >
              <ChevronRight className="w-5 h-5 rotate-180" />
            </button>
            <BookOpen className="w-5 h-5 text-sky-600" />
            <div>
              <h2 className="text-sm font-semibold text-stone-800">{selectedDoc.title}</h2>
              <p className="text-xs text-stone-500">{selectedDoc.name}</p>
            </div>
          </div>
          {onClose && (
            <button
              type="button"
              onClick={onClose}
              className="p-1.5 rounded-lg hover:bg-stone-100 text-stone-400"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto p-4">
          {loading ? (
            <div className="flex items-center justify-center h-32">
              <div className="text-sm text-stone-500">Loading...</div>
            </div>
          ) : (
            <div className="prose prose-sm max-w-none prose-stone">
              <ReactMarkdown rehypePlugins={[rehypeRaw]} components={components}>
                {content}
              </ReactMarkdown>
            </div>
          )}
        </div>
      </div>
    );
  }

  // Document list view
  return (
    <div className="card p-4">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-2">
          <BookOpen className="w-5 h-5 text-sky-600" />
          <h2 className="text-sm font-semibold text-stone-800">Documentation</h2>
        </div>
        {onClose && (
          <button
            type="button"
            onClick={onClose}
            className="p-1.5 rounded-lg hover:bg-stone-100 text-stone-400"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      {docs.length === 0 ? (
        <div className="text-center py-8 text-stone-500">
          <FileText className="w-12 h-12 mx-auto mb-3 opacity-50" />
          <p className="text-sm">No documentation files found</p>
          <p className="text-xs mt-1">Add .md files to .narrative/ directory</p>
        </div>
      ) : (
        <div className="space-y-2">
          {docs.map((doc) => (
            <button
              key={doc.path}
              type="button"
              onClick={() => setSelectedDoc(doc)}
              className="w-full flex items-center gap-3 p-3 rounded-lg border border-stone-200 hover:border-sky-300 hover:bg-sky-50 transition-all text-left group"
            >
              <FileText className="w-5 h-5 text-stone-400 group-hover:text-sky-500" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-stone-700 group-hover:text-sky-700 truncate">
                  {doc.title}
                </p>
                <p className="text-xs text-stone-400 truncate">{doc.name}</p>
              </div>
              <ChevronRight className="w-4 h-4 text-stone-300 group-hover:text-sky-400" />
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
