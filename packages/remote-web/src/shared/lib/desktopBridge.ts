import type {
  OpenRemoteEditorWithStoredCredentialsRequest,
  UpsertOpenRemoteEditorCredentialsRequest,
} from "shared/types";

const BRIDGE_PORT = 15147;

function getBridgeUrl(): string {
  return `http://127.0.0.1:${BRIDGE_PORT}`;
}

export async function openRemoteEditor(
  request: OpenRemoteEditorWithStoredCredentialsRequest,
): Promise<string | null> {
  const response = await fetch(`${getBridgeUrl()}/api/open-remote-editor`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(request),
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(body.error || `Desktop bridge error (${response.status})`);
  }

  const data = (await response.json()) as { url?: string };
  return data.url ?? null;
}

export async function upsertOpenRemoteEditorCredentials(
  request: UpsertOpenRemoteEditorCredentialsRequest,
): Promise<void> {
  const response = await fetch(
    `${getBridgeUrl()}/api/open-remote-editor/credentials`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(request),
    },
  );

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(body.error || `Desktop bridge error (${response.status})`);
  }
}
