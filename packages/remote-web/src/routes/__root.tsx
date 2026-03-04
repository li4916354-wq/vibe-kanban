import { type ReactNode, useCallback, useEffect, useMemo } from "react";
import {
  createRootRoute,
  Outlet,
  useLocation,
  useParams,
} from "@tanstack/react-router";
import { Provider as NiceModalProvider } from "@ebay/nice-modal-react";
import { useSystemTheme } from "@remote/shared/hooks/useSystemTheme";
import { RemoteActionsProvider } from "@remote/app/providers/RemoteActionsProvider";
import { RemoteUserSystemProvider } from "@remote/app/providers/RemoteUserSystemProvider";
import { RemoteAppShell } from "@remote/app/layout/RemoteAppShell";
import { UserProvider } from "@/shared/providers/remote/UserProvider";
import { WorkspaceProvider } from "@/shared/providers/WorkspaceProvider";
import { ExecutionProcessesProvider } from "@/shared/providers/ExecutionProcessesProvider";
import { TerminalProvider } from "@/shared/providers/TerminalProvider";
import { LogsPanelProvider } from "@/shared/providers/LogsPanelProvider";
import { ActionsProvider } from "@/shared/providers/ActionsProvider";
import { ActionsContext } from "@/shared/hooks/useActions";
import { useActions } from "@/shared/hooks/useActions";
import type { ActionDefinition } from "@/shared/types/actions";
import { useAuth } from "@/shared/hooks/auth/useAuth";
import { useUserSystem } from "@/shared/hooks/useUserSystem";
import { useWorkspaceContext } from "@/shared/hooks/useWorkspaceContext";
import type { OpenRemoteEditorRequest } from "shared/types";
import { AppNavigationProvider } from "@/shared/hooks/useAppNavigation";
import {
  SequenceTrackerProvider,
  SequenceIndicator,
  useWorkspaceShortcuts,
  useIssueShortcuts,
  useKeyShowHelp,
  Scope,
} from "@/shared/keyboard";
import { KeyboardShortcutsDialog } from "@/shared/dialogs/shared/KeyboardShortcutsDialog";
import {
  createRemoteHostAppNavigation,
  remoteFallbackAppNavigation,
  resolveRemoteDestinationFromPath,
} from "@remote/app/navigation/AppNavigation";
import {
  resolveRelayNavigationHostId,
  useRelayAppBarHosts,
} from "@remote/shared/hooks/useRelayAppBarHosts";
import { setActiveRelayHostId } from "@remote/shared/lib/relay/activeHostContext";
import {
  isProjectDestination,
  isWorkspacesDestination,
} from "@/shared/lib/routes/appNavigation";
import { attemptsApi } from "@/shared/lib/api";
import { openRemoteEditor } from "@remote/shared/lib/desktopBridge";
import { resolveRelayHostContext } from "@remote/shared/lib/relay/context";
import NotFoundPage from "../pages/NotFoundPage";

export const Route = createRootRoute({
  component: RootLayout,
  notFoundComponent: NotFoundPage,
});

function ExecutionProcessesProviderWrapper({
  children,
}: {
  children: ReactNode;
}) {
  const { selectedSessionId } = useWorkspaceContext();

  return (
    <ExecutionProcessesProvider sessionId={selectedSessionId}>
      {children}
    </ExecutionProcessesProvider>
  );
}

/**
 * Global keyboard shortcut that doesn't require workspace/actions context.
 * Renders inside HotkeysProvider (from App.tsx) but outside WorkspaceProvider.
 */
function GlobalKeyboardShortcuts() {
  useKeyShowHelp(
    () => {
      KeyboardShortcutsDialog.show();
    },
    { scope: Scope.GLOBAL },
  );
  return null;
}

/**
 * Workspace & issue keyboard shortcuts that require ActionsProvider + WorkspaceProvider.
 * Must be rendered inside WorkspaceRouteProviders.
 */
function WorkspaceKeyboardShortcuts() {
  useWorkspaceShortcuts();
  useIssueShortcuts();
  return null;
}

/**
 * Thin override layer inside ActionsProvider that intercepts specific actions
 * (e.g. open-in-ide) with remote-specific handling (desktop bridge + relay tunnel)
 * while delegating everything else to the inner ActionsProvider.
 */
function RemoteActionOverrides({ children }: { children: ReactNode }) {
  const inner = useActions();
  const { hostId, workspaceId } = useParams({ strict: false });
  const { config } = useUserSystem();
  const editorType = config?.editor?.editor_type;

  const executeAction = useCallback(
    async (
      action: ActionDefinition,
      wsId?: string,
      repoIdOrProjectId?: string,
      issueIds?: string[],
    ): Promise<void> => {
      if (action.id === "open-in-ide") {
        if (!workspaceId || !hostId) return;
        try {
          const [{ workspace_path }, relayCtx] = await Promise.all([
            attemptsApi.getEditorPath(workspaceId),
            resolveRelayHostContext(hostId),
          ]);
          const request: OpenRemoteEditorRequest = {
            workspace_path,
            editor_type: editorType ?? null,
            relay_session_base_url: relayCtx.relaySessionBaseUrl,
            signing_session_id: relayCtx.pairedHost.signing_session_id!,
            private_key_jwk: relayCtx.pairedHost
              .private_key_jwk as OpenRemoteEditorRequest["private_key_jwk"],
          };
          const url = await openRemoteEditor(request);
          if (url) {
            window.open(url, "_blank");
          }
        } catch (err) {
          console.error("[RemoteActionOverrides] Open in IDE failed:", err);
        }
        return;
      }

      return inner.executeAction(action, wsId, repoIdOrProjectId, issueIds);
    },
    [inner.executeAction, workspaceId, hostId, editorType],
  );

  const value = useMemo(
    () => ({ ...inner, executeAction }),
    [inner, executeAction],
  );

  return (
    <ActionsContext.Provider value={value}>{children}</ActionsContext.Provider>
  );
}

function WorkspaceRouteProviders({ children }: { children: ReactNode }) {
  return (
    <WorkspaceProvider>
      <ExecutionProcessesProviderWrapper>
        <TerminalProvider>
          <LogsPanelProvider>
            <ActionsProvider>
              <RemoteActionOverrides>
                <WorkspaceKeyboardShortcuts />
                {children}
              </RemoteActionOverrides>
            </ActionsProvider>
          </LogsPanelProvider>
        </TerminalProvider>
      </ExecutionProcessesProviderWrapper>
    </WorkspaceProvider>
  );
}

function RootLayout() {
  useSystemTheme();
  const { isSignedIn } = useAuth();
  const location = useLocation();
  const { hostId } = useParams({ strict: false });
  const routeHostId = hostId ?? null;
  const { hosts: relayHosts } = useRelayAppBarHosts(isSignedIn);
  const navigationHostId = useMemo(
    () => resolveRelayNavigationHostId(relayHosts, { routeHostId }),
    [relayHosts, routeHostId],
  );

  useEffect(() => {
    setActiveRelayHostId(navigationHostId);
  }, [navigationHostId]);

  const appNavigation = useMemo(
    () =>
      navigationHostId
        ? createRemoteHostAppNavigation(navigationHostId)
        : remoteFallbackAppNavigation,
    [navigationHostId],
  );
  const isStandaloneRoute =
    location.pathname.startsWith("/account") ||
    location.pathname.startsWith("/login") ||
    location.pathname.startsWith("/upgrade") ||
    location.pathname.startsWith("/invitations");
  const destination = resolveRemoteDestinationFromPath(location.pathname);
  const isWorkspaceProviderRoute =
    isProjectDestination(destination) || isWorkspacesDestination(destination);

  const pageContent = isStandaloneRoute ? (
    <Outlet />
  ) : (
    <SequenceTrackerProvider>
      <SequenceIndicator />
      <GlobalKeyboardShortcuts />
      <RemoteAppShell>
        <Outlet />
      </RemoteAppShell>
    </SequenceTrackerProvider>
  );

  const content = isWorkspaceProviderRoute ? (
    <WorkspaceRouteProviders>
      <NiceModalProvider>{pageContent}</NiceModalProvider>
    </WorkspaceRouteProviders>
  ) : (
    <NiceModalProvider>{pageContent}</NiceModalProvider>
  );

  return (
    <AppNavigationProvider value={appNavigation}>
      <UserProvider>
        <RemoteActionsProvider>
          <RemoteUserSystemProvider>{content}</RemoteUserSystemProvider>
        </RemoteActionsProvider>
      </UserProvider>
    </AppNavigationProvider>
  );
}
