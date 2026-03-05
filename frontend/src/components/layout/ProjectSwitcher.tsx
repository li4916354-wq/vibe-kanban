import { useCallback, useState, useMemo } from 'react';
import { useNavigate } from 'react-router-dom';
import { ChevronDown, Plus } from 'lucide-react';
import { cn } from '@/lib/utils';
import { useProjects } from '@/hooks/useProjects';
import { useProject } from '@/contexts/ProjectContext';
import { ProjectFormDialog } from '@/components/dialogs/projects/ProjectFormDialog';

export function ProjectSwitcher() {
  const navigate = useNavigate();
  const { projectId, project } = useProject();
  const { projects } = useProjects();
  const [isOpen, setIsOpen] = useState(false);

  const handleProjectSelect = useCallback(
    (selectedProjectId: string) => {
      navigate(`/projects/${selectedProjectId}/tasks`);
      setIsOpen(false);
    },
    [navigate]
  );

  const handleCreateProject = useCallback(async () => {
    const result = await ProjectFormDialog.show();
    if (result.status === 'saved') {
      navigate(`/projects/${result.project.id}/tasks`);
    }
    setIsOpen(false);
  }, [navigate]);

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className={cn(
          'hidden sm:inline-flex items-center ml-3 text-sm font-medium overflow-hidden border h-6',
          'hover:bg-muted/50 transition-colors',
          'focus:outline-none focus:ring-1 focus:ring-accent'
        )}
      >
        <span className="bg-muted text-foreground flex items-center px-2 py-1 border-r">
          <svg
            className="h-4 w-4"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
          >
            <path d="M3 7v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-6l-2-2H5a2 2 0 0 0-2 2z" />
          </svg>
        </span>
        <span className="h-full items-center flex px-2 text-xs">
          {project?.name ?? 'Select Project'}
        </span>
        <span className="px-1 border-l">
          <ChevronDown className="h-3 w-3" />
        </span>
      </button>

      {isOpen && (
        <>
          <div
            className="fixed inset-0 z-40"
            onClick={() => setIsOpen(false)}
          />
          <div className="absolute top-full left-0 mt-1 z-50 w-72 bg-popover border rounded-md shadow-lg">
            <div className="max-h-96 overflow-auto p-1">
              <button
                type="button"
                onClick={handleCreateProject}
                className="w-full flex items-center gap-2 px-3 py-2 text-sm text-left hover:bg-accent rounded-md transition-colors"
              >
                <Plus className="h-4 w-4 text-accent" />
                <span className="text-accent font-medium">Create new project</span>
              </button>
              <div className="my-1 border-t" />
              {projects.length === 0 ? (
                <div className="px-3 py-2 text-sm text-muted-foreground text-center">
                  No projects yet
                </div>
              ) : (
                projects.map((p) => (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() => handleProjectSelect(p.id)}
                    className={cn(
                      'w-full flex items-center gap-2 px-3 py-2 text-sm text-left rounded-md transition-colors',
                      'hover:bg-accent',
                      projectId === p.id && 'bg-accent'
                    )}
                  >
                    <svg
                      className="h-4 w-4 shrink-0 text-muted-foreground"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2"
                    >
                      <path d="M3 7v10a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-6l-2-2H5a2 2 0 0 0-2 2z" />
                    </svg>
                    <span className="truncate">{p.name}</span>
                  </button>
                ))
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
