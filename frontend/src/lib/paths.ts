export const paths = {
  projects: () => '/projects',
  projectTasks: (projectId: string) => `/projects/${projectId}/tasks`,
  projectChat: (projectId: string) => `/projects/${projectId}/chat`,
  projectChatSession: (projectId: string, sessionId: string) =>
    `/projects/${projectId}/chat/${sessionId}`,
  projectFiles: (projectId: string) => `/projects/${projectId}/files`,
  task: (projectId: string, taskId: string) =>
    `/projects/${projectId}/tasks/${taskId}`,
  attempt: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/tasks/${taskId}/attempts/${attemptId}`,
  attemptFull: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/tasks/${taskId}/attempts/${attemptId}/full`,
};
