import { useCallback } from 'react';
import { useNavigate, useLocation } from 'react-router-dom';
import { cn } from '@/lib/utils';
import { useProject } from '@/contexts/ProjectContext';
import { LayoutGrid, MessageSquare, FolderTree } from 'lucide-react';

export type ProjectTab = 'kanban' | 'chat' | 'files';

interface TabConfig {
  id: ProjectTab;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  path: (projectId: string) => string;
}

const TABS: TabConfig[] = [
  {
    id: 'kanban',
    label: '看板',
    icon: LayoutGrid,
    path: (projectId) => `/projects/${projectId}/tasks`,
  },
  {
    id: 'chat',
    label: '聊天',
    icon: MessageSquare,
    path: (projectId) => `/projects/${projectId}/chat`,
  },
  {
    id: 'files',
    label: '文件',
    icon: FolderTree,
    path: (projectId) => `/projects/${projectId}/files`,
  },
];

function getActiveTab(pathname: string, projectId: string | null | undefined): ProjectTab {
  if (!projectId) return 'kanban';
  if (pathname.includes(`/projects/${projectId}/chat`)) return 'chat';
  if (pathname.includes(`/projects/${projectId}/files`)) return 'files';
  return 'kanban';
}

export function ProjectTabSwitcher() {
  const navigate = useNavigate();
  const location = useLocation();
  const { projectId } = useProject();

  const activeTab = getActiveTab(location.pathname, projectId);

  const handleTabChange = useCallback(
    (tab: ProjectTab) => {
      if (!projectId) return;
      const tabConfig = TABS.find((t) => t.id === tab);
      if (tabConfig) {
        navigate(tabConfig.path(projectId));
      }
    },
    [projectId, navigate]
  );

  if (!projectId) return null;

  return (
    <div className="flex items-center ml-2 border rounded-md overflow-hidden h-6">
      {TABS.map((tab) => {
        const Icon = tab.icon;
        const isActive = activeTab === tab.id;
        return (
          <button
            key={tab.id}
            type="button"
            onClick={() => handleTabChange(tab.id)}
            className={cn(
              'flex items-center gap-1 px-2 py-1 text-xs font-medium transition-colors',
              'hover:bg-muted/50',
              'focus:outline-none focus:ring-1 focus:ring-accent focus:ring-inset',
              isActive && 'bg-accent text-accent-foreground',
              !isActive && 'text-muted-foreground'
            )}
          >
            <Icon className="h-3 w-3" />
            <span className="hidden sm:inline">{tab.label}</span>
          </button>
        );
      })}
    </div>
  );
}
