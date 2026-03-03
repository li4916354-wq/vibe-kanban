const DEFAULT_BRIDGE_PORT = 15147;
const BRIDGE_PORT_KEY = "vk-desktop-bridge-port";

function getBridgePort(): number {
  try {
    const stored = localStorage.getItem(BRIDGE_PORT_KEY);
    if (stored) {
      const port = parseInt(stored, 10);
      if (port > 0 && port < 65536) return port;
    }
  } catch {
    // localStorage unavailable
  }
  return DEFAULT_BRIDGE_PORT;
}

function getBridgeUrl(): string {
  return `http://127.0.0.1:${getBridgePort()}`;
}

export interface OpenRemoteEditorRequest {
  workspace_path: string;
  editor_type?: string;
  /** Relay proxy session URL (e.g. https://relay.example.com/relay/h/{host_id}/s/{session_id}) */
  relay_session_base_url: string;
  /** Ed25519 signing session ID */
  signing_session_id: string;
  /** Ed25519 private key in JWK format */
  private_key_jwk: JsonWebKey;
}

export async function openRemoteEditor(
  request: OpenRemoteEditorRequest,
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

export async function isDesktopBridgeAvailable(): Promise<boolean> {
  try {
    const response = await fetch(`${getBridgeUrl()}/api/health`, {
      signal: AbortSignal.timeout(2000),
    });
    return response.ok;
  } catch {
    return false;
  }
}
