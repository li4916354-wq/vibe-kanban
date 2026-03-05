import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { chatSessionsApi } from '@/lib/api';

export const chatSessionKeys = {
  all: ['chat-sessions'] as const,
  list: (projectId: string) => [...chatSessionKeys.all, 'list', projectId] as const,
  detail: (sessionId: string) => [...chatSessionKeys.all, 'detail', sessionId] as const,
};

export function useChatSessions(projectId: string | undefined) {
  return useQuery({
    queryKey: chatSessionKeys.list(projectId ?? ''),
    queryFn: () => chatSessionsApi.list(projectId!),
    enabled: !!projectId,
    refetchInterval: 5000, // Poll every 5 seconds to update running status
  });
}

export function useChatSession(sessionId: string | undefined) {
  return useQuery({
    queryKey: chatSessionKeys.detail(sessionId ?? ''),
    queryFn: () => chatSessionsApi.get(sessionId!),
    enabled: !!sessionId,
  });
}

export function useCreateChatSession() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (data: { project_id: string; title?: string; executor?: string }) =>
      chatSessionsApi.create(data),
    onSuccess: (newSession) => {
      queryClient.invalidateQueries({
        queryKey: chatSessionKeys.list(newSession.chat_session.project_id),
      });
    },
  });
}

export function useUpdateChatSession() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({
      sessionId,
      data,
    }: {
      sessionId: string;
      data: { title?: string; pinned?: boolean };
    }) => chatSessionsApi.update(sessionId, data),
    onSuccess: (updatedSession) => {
      queryClient.invalidateQueries({
        queryKey: chatSessionKeys.list(updatedSession.chat_session.project_id),
      });
      queryClient.invalidateQueries({
        queryKey: chatSessionKeys.detail(updatedSession.chat_session.id),
      });
    },
  });
}

export function useDeleteChatSession() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ sessionId, projectId }: { sessionId: string; projectId: string }) =>
      chatSessionsApi.delete(sessionId).then(() => projectId),
    onSuccess: (projectId) => {
      queryClient.invalidateQueries({
        queryKey: chatSessionKeys.list(projectId),
      });
    },
  });
}
