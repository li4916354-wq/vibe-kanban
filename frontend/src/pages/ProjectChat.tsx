import { useCallback, useEffect, useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import { Plus, MoreHorizontal, Pin, PinOff, Trash2, Loader2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { cn } from '@/lib/utils';
import {
  useChatSessions,
  useCreateChatSession,
  useUpdateChatSession,
  useDeleteChatSession,
} from '@/hooks/useChatSessions';
import { paths } from '@/lib/paths';
import type { ChatSessionWithStatus } from '@/lib/api';

function formatElapsedTime(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const secs = seconds % 60;
  if (minutes < 60) return `${minutes}m ${secs}s`;
  const hours = Math.floor(minutes / 60);
  const mins = minutes % 60;
  return `${hours}h ${mins}m`;
}

function formatTime(dateStr: string): string {
  const date = new Date(dateStr);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  const diffHours = Math.floor(diffMins / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffMins < 1) return '刚刚';
  if (diffMins < 60) return `${diffMins}分钟前`;
  if (diffHours < 24) return `${diffHours}小时前`;
  if (diffDays < 7) return `${diffDays}天前`;
  return date.toLocaleDateString();
}

interface ChatSessionItemProps {
  session: ChatSessionWithStatus;
  isSelected: boolean;
  onSelect: () => void;
  onPin: () => void;
  onDelete: () => void;
}

function ChatSessionItem({
  session,
  isSelected,
  onSelect,
  onPin,
  onDelete,
}: ChatSessionItemProps) {
  const [showMenu, setShowMenu] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(
    session.elapsed_seconds ?? 0
  );

  // Update elapsed time every second when running
  useEffect(() => {
    if (!session.is_running) {
      setElapsedSeconds(session.elapsed_seconds ?? 0);
      return;
    }

    setElapsedSeconds(session.elapsed_seconds ?? 0);
    const interval = setInterval(() => {
      setElapsedSeconds((prev) => prev + 1);
    }, 1000);

    return () => clearInterval(interval);
  }, [session.is_running, session.elapsed_seconds]);

  return (
    <div
      className={cn(
        'group relative flex items-center gap-2 px-3 py-2 rounded-md cursor-pointer transition-colors',
        'hover:bg-accent',
        isSelected && 'bg-accent'
      )}
      onClick={onSelect}
      onMouseEnter={() => setShowMenu(true)}
      onMouseLeave={() => setShowMenu(false)}
    >
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          {session.chat_session.pinned && (
            <Pin className="h-3 w-3 text-muted-foreground shrink-0" />
          )}
          <span className="truncate text-sm font-medium">
            {session.chat_session.title || '新会话'}
          </span>
        </div>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          {session.is_running ? (
            <span className="flex items-center gap-1 text-primary">
              <Loader2 className="h-3 w-3 animate-spin" />
              {formatElapsedTime(elapsedSeconds)}
            </span>
          ) : (
            <span>{formatTime(session.chat_session.updated_at)}</span>
          )}
        </div>
      </div>

      {(showMenu || isSelected) && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild onClick={(e) => e.stopPropagation()}>
            <Button
              variant="ghost"
              size="icon"
              className="h-6 w-6 shrink-0 opacity-0 group-hover:opacity-100"
            >
              <MoreHorizontal className="h-4 w-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
            <DropdownMenuItem onClick={onPin}>
              {session.chat_session.pinned ? (
                <>
                  <PinOff className="h-4 w-4 mr-2" />
                  取消固定
                </>
              ) : (
                <>
                  <Pin className="h-4 w-4 mr-2" />
                  固定
                </>
              )}
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={onDelete}
              className="text-destructive focus:text-destructive"
            >
              <Trash2 className="h-4 w-4 mr-2" />
              删除
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </div>
  );
}

export function ProjectChat() {
  const navigate = useNavigate();
  const { projectId, sessionId } = useParams<{
    projectId: string;
    sessionId?: string;
  }>();

  const { data: sessions = [], isLoading } = useChatSessions(projectId);
  const createSession = useCreateChatSession();
  const updateSession = useUpdateChatSession();
  const deleteSession = useDeleteChatSession();

  const selectedSession = useMemo(
    () => sessions.find((s) => s.chat_session.id === sessionId),
    [sessions, sessionId]
  );

  // Auto-select first session if none selected
  useEffect(() => {
    if (!sessionId && sessions.length > 0 && projectId) {
      navigate(paths.projectChatSession(projectId, sessions[0].chat_session.id), {
        replace: true,
      });
    }
  }, [sessionId, sessions, projectId, navigate]);

  const handleCreateSession = useCallback(async () => {
    if (!projectId) return;
    const newSession = await createSession.mutateAsync({
      project_id: projectId,
    });
    navigate(paths.projectChatSession(projectId, newSession.chat_session.id));
  }, [projectId, createSession, navigate]);

  const handleSelectSession = useCallback(
    (id: string) => {
      if (projectId) {
        navigate(paths.projectChatSession(projectId, id));
      }
    },
    [projectId, navigate]
  );

  const handlePinSession = useCallback(
    async (session: ChatSessionWithStatus) => {
      await updateSession.mutateAsync({
        sessionId: session.chat_session.id,
        data: { pinned: !session.chat_session.pinned },
      });
    },
    [updateSession]
  );

  const handleDeleteSession = useCallback(
    async (session: ChatSessionWithStatus) => {
      if (!projectId) return;

      const isCurrentSession = session.chat_session.id === sessionId;
      await deleteSession.mutateAsync({
        sessionId: session.chat_session.id,
        projectId,
      });

      // If deleted current session, select first remaining session
      if (isCurrentSession) {
        const remaining = sessions.filter(
          (s) => s.chat_session.id !== session.chat_session.id
        );
        if (remaining.length > 0) {
          navigate(
            paths.projectChatSession(projectId, remaining[0].chat_session.id),
            { replace: true }
          );
        } else {
          navigate(paths.projectChat(projectId), { replace: true });
        }
      }
    },
    [projectId, sessionId, sessions, deleteSession, navigate]
  );

  return (
    <div className="h-full flex">
      {/* Left sidebar - session list */}
      <div className="w-64 border-r flex flex-col bg-muted/30">
        <div className="p-3 border-b">
          <Button
            variant="outline"
            size="sm"
            className="w-full justify-start gap-2"
            onClick={handleCreateSession}
            disabled={createSession.isPending}
          >
            <Plus className="h-4 w-4" />
            新建会话
          </Button>
        </div>

        <div className="flex-1 overflow-y-auto p-2">
          {isLoading ? (
            <div className="flex items-center justify-center py-8">
              <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
            </div>
          ) : sessions.length === 0 ? (
            <div className="text-center py-8 text-sm text-muted-foreground">
              暂无会话
            </div>
          ) : (
            <div className="space-y-1">
              {sessions.map((session) => (
                <ChatSessionItem
                  key={session.chat_session.id}
                  session={session}
                  isSelected={session.chat_session.id === sessionId}
                  onSelect={() => handleSelectSession(session.chat_session.id)}
                  onPin={() => handlePinSession(session)}
                  onDelete={() => handleDeleteSession(session)}
                />
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Right side - chat window */}
      <div className="flex-1 flex flex-col">
        {selectedSession ? (
          <div className="flex-1 flex items-center justify-center text-muted-foreground">
            {/* TODO: Integrate with existing conversation components */}
            <div className="text-center">
              <p className="text-lg font-medium">
                {selectedSession.chat_session.title || '新会话'}
              </p>
              <p className="text-sm mt-2">
                会话窗口开发中...
              </p>
            </div>
          </div>
        ) : (
          <div className="flex-1 flex items-center justify-center text-muted-foreground">
            <div className="text-center">
              <p>选择或创建一个会话开始聊天</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
