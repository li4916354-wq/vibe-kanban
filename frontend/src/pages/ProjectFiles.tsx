import { useCallback, useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import {
  ChevronRight,
  ChevronDown,
  File,
  Folder,
  FolderOpen,
  Loader2,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { useProjectRepos } from '@/hooks';

interface FileNode {
  name: string;
  path: string;
  type: 'file' | 'directory';
  children?: FileNode[];
}

interface FileTreeItemProps {
  node: FileNode;
  level: number;
  selectedPath: string | null;
  expandedPaths: Set<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
}

function FileTreeItem({
  node,
  level,
  selectedPath,
  expandedPaths,
  onSelect,
  onToggle,
}: FileTreeItemProps) {
  const isExpanded = expandedPaths.has(node.path);
  const isSelected = selectedPath === node.path;
  const isDirectory = node.type === 'directory';

  const handleClick = () => {
    if (isDirectory) {
      onToggle(node.path);
    } else {
      onSelect(node.path);
    }
  };

  return (
    <div>
      <div
        className={cn(
          'flex items-center gap-1 px-2 py-1 cursor-pointer rounded-sm transition-colors',
          'hover:bg-accent',
          isSelected && 'bg-accent'
        )}
        style={{ paddingLeft: `${level * 12 + 8}px` }}
        onClick={handleClick}
      >
        {isDirectory ? (
          <>
            {isExpanded ? (
              <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground" />
            )}
            {isExpanded ? (
              <FolderOpen className="h-4 w-4 shrink-0 text-yellow-500" />
            ) : (
              <Folder className="h-4 w-4 shrink-0 text-yellow-500" />
            )}
          </>
        ) : (
          <>
            <span className="w-4" />
            <File className="h-4 w-4 shrink-0 text-muted-foreground" />
          </>
        )}
        <span className="truncate text-sm">{node.name}</span>
      </div>

      {isDirectory && isExpanded && node.children && (
        <div>
          {node.children.map((child) => (
            <FileTreeItem
              key={child.path}
              node={child}
              level={level + 1}
              selectedPath={selectedPath}
              expandedPaths={expandedPaths}
              onSelect={onSelect}
              onToggle={onToggle}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function getFileExtension(filename: string): string {
  const lastDot = filename.lastIndexOf('.');
  if (lastDot === -1) return '';
  return filename.slice(lastDot + 1).toLowerCase();
}

function getLanguageFromExtension(ext: string): string {
  const languageMap: Record<string, string> = {
    js: 'javascript',
    jsx: 'javascript',
    ts: 'typescript',
    tsx: 'typescript',
    py: 'python',
    rb: 'ruby',
    rs: 'rust',
    go: 'go',
    java: 'java',
    kt: 'kotlin',
    swift: 'swift',
    c: 'c',
    cpp: 'cpp',
    h: 'c',
    hpp: 'cpp',
    cs: 'csharp',
    php: 'php',
    html: 'html',
    css: 'css',
    scss: 'scss',
    less: 'less',
    json: 'json',
    yaml: 'yaml',
    yml: 'yaml',
    xml: 'xml',
    md: 'markdown',
    sql: 'sql',
    sh: 'bash',
    bash: 'bash',
    zsh: 'bash',
    dockerfile: 'dockerfile',
    toml: 'toml',
    ini: 'ini',
    env: 'plaintext',
    txt: 'plaintext',
  };
  return languageMap[ext] || 'plaintext';
}

interface FilePreviewProps {
  path: string;
  repoPath: string;
}

function FilePreview({ path, repoPath }: FilePreviewProps) {
  const [content, setContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchContent = async () => {
      setLoading(true);
      setError(null);
      try {
        // Use the filesystem API to read file content
        const fullPath = `${repoPath}/${path}`;
        const response = await fetch(
          `/api/filesystem/file?path=${encodeURIComponent(fullPath)}`
        );
        if (!response.ok) {
          throw new Error('Failed to load file');
        }
        const data = await response.json();
        setContent(data.data?.content ?? '');
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load file');
      } finally {
        setLoading(false);
      }
    };

    fetchContent();
  }, [path, repoPath]);

  const ext = getFileExtension(path);
  // Language detection for future syntax highlighting
  void getLanguageFromExtension(ext);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full text-destructive">
        {error}
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      <div className="px-4 py-2 border-b bg-muted/30 text-sm font-medium">
        {path}
      </div>
      <div className="flex-1 overflow-auto">
        <pre className="p-4 text-sm font-mono whitespace-pre-wrap break-all">
          <code>{content}</code>
        </pre>
      </div>
    </div>
  );
}

export function ProjectFiles() {
  const { projectId } = useParams<{ projectId: string }>();
  const { data: repos = [] } = useProjectRepos(projectId);

  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [fileTree, setFileTree] = useState<FileNode[]>([]);
  const [loading, setLoading] = useState(true);

  // Get the first repo's path for file operations
  const repoPath = repos[0]?.path ?? '';

  // Fetch file tree from the first repo
  useEffect(() => {
    if (!repoPath) {
      setLoading(false);
      return;
    }

    const fetchFileTree = async () => {
      setLoading(true);
      try {
        const response = await fetch(
          `/api/filesystem/tree?path=${encodeURIComponent(repoPath)}&depth=3`
        );
        if (response.ok) {
          const data = await response.json();
          setFileTree(data.data?.entries ?? []);
        }
      } catch (err) {
        console.error('Failed to fetch file tree:', err);
      } finally {
        setLoading(false);
      }
    };

    fetchFileTree();
  }, [repoPath]);

  const handleSelect = useCallback((path: string) => {
    setSelectedPath(path);
  }, []);

  const handleToggle = useCallback((path: string) => {
    setExpandedPaths((prev) => {
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
    <div className="h-full flex">
      {/* Left sidebar - file tree */}
      <div className="w-64 border-r flex flex-col bg-muted/30">
        <div className="p-3 border-b">
          <h3 className="text-sm font-medium">文件</h3>
        </div>

        <div className="flex-1 overflow-auto">
          {loading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
          ) : fileTree.length === 0 ? (
            <div className="text-center py-8 text-sm text-muted-foreground">
              {repoPath ? '暂无文件' : '请先添加仓库'}
            </div>
          ) : (
            <div className="py-2">
              {fileTree.map((node) => (
                <FileTreeItem
                  key={node.path}
                  node={node}
                  level={0}
                  selectedPath={selectedPath}
                  expandedPaths={expandedPaths}
                  onSelect={handleSelect}
                  onToggle={handleToggle}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Right side - file preview */}
      <div className="flex-1 flex flex-col">
        {selectedPath && repoPath ? (
          <FilePreview path={selectedPath} repoPath={repoPath} />
        ) : (
          <div className="flex-1 flex items-center justify-center text-muted-foreground">
            <div className="text-center">
              <p>选择一个文件查看内容</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
