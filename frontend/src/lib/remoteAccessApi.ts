/**
 * Remote Access API
 * Client-side API for remote access, tunnel, and TOTP functionality
 */

export interface RemoteAccessStatus {
  enabled: boolean;
  server: {
    running: boolean;
    port: number;
    token: string | null;
    localUrl: string | null;
    lanUrl: string | null;
  };
  tunnel: {
    status: 'stopped' | 'starting' | 'running' | 'error';
    url: string | null;
    error: string | null;
  };
  clients: number;
}

export interface TotpStatus {
  bound: boolean;
  secret?: string;
}

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

type StatusChangeCallback = (status: RemoteAccessStatus) => void;

let statusChangeCallbacks: StatusChangeCallback[] = [];
let eventSource: EventSource | null = null;

// Subscribe to SSE for real-time status updates
function setupEventSource() {
  if (eventSource) return;

  eventSource = new EventSource('/api/remote/events');

  eventSource.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data) as RemoteAccessStatus;
      statusChangeCallbacks.forEach((cb) => cb(data));
    } catch (error) {
      console.error('[RemoteAccessApi] Failed to parse SSE data:', error);
    }
  };

  eventSource.onerror = () => {
    // Reconnect on error
    eventSource?.close();
    eventSource = null;
    setTimeout(setupEventSource, 5000);
  };
}

export const remoteAccessApi = {
  /**
   * Get current remote access status
   */
  getStatus: async (): Promise<ApiResponse<RemoteAccessStatus>> => {
    try {
      const response = await fetch('/api/remote/status');
      const result = await response.json();
      return result;
    } catch (error) {
      console.error('[RemoteAccessApi] getStatus error:', error);
      return { success: false, error: 'Failed to get status' };
    }
  },

  /**
   * Enable remote access
   */
  enable: async (port?: number): Promise<ApiResponse<RemoteAccessStatus>> => {
    try {
      const response = await fetch('/api/remote/enable', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ port }),
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] enable error:', error);
      return { success: false, error: 'Failed to enable remote access' };
    }
  },

  /**
   * Disable remote access
   */
  disable: async (): Promise<ApiResponse<void>> => {
    try {
      const response = await fetch('/api/remote/disable', {
        method: 'POST',
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] disable error:', error);
      return { success: false, error: 'Failed to disable remote access' };
    }
  },

  /**
   * Enable tunnel for internet access
   */
  enableTunnel: async (): Promise<ApiResponse<{ url: string }>> => {
    try {
      const response = await fetch('/api/remote/tunnel/enable', {
        method: 'POST',
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] enableTunnel error:', error);
      return { success: false, error: 'Failed to enable tunnel' };
    }
  },

  /**
   * Disable tunnel
   */
  disableTunnel: async (): Promise<ApiResponse<void>> => {
    try {
      const response = await fetch('/api/remote/tunnel/disable', {
        method: 'POST',
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] disableTunnel error:', error);
      return { success: false, error: 'Failed to disable tunnel' };
    }
  },

  /**
   * Get QR code for remote access
   */
  getQRCode: async (
    includeToken?: boolean
  ): Promise<ApiResponse<{ qrCode: string }>> => {
    try {
      const params = includeToken ? '?include_token=true' : '';
      const response = await fetch(`/api/remote/qrcode${params}`);
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] getQRCode error:', error);
      return { success: false, error: 'Failed to get QR code' };
    }
  },

  /**
   * Set custom password for remote access
   */
  setPassword: async (password: string): Promise<ApiResponse<void>> => {
    try {
      const response = await fetch('/api/remote/password', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ password }),
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] setPassword error:', error);
      return { success: false, error: 'Failed to set password' };
    }
  },

  /**
   * Get TOTP binding status
   */
  getTotpStatus: async (): Promise<ApiResponse<TotpStatus>> => {
    try {
      const response = await fetch('/api/remote/totp/status');
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] getTotpStatus error:', error);
      return { success: false, error: 'Failed to get TOTP status' };
    }
  },

  /**
   * Generate TOTP secret and QR code for binding
   */
  generateTotpSecret: async (): Promise<
    ApiResponse<{ secret: string; qrCode: string }>
  > => {
    try {
      const response = await fetch('/api/remote/totp/generate', {
        method: 'POST',
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] generateTotpSecret error:', error);
      return { success: false, error: 'Failed to generate TOTP secret' };
    }
  },

  /**
   * Bind TOTP with verification code
   */
  bindTotp: async (code: string): Promise<ApiResponse<void>> => {
    try {
      const response = await fetch('/api/remote/totp/bind', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ code }),
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] bindTotp error:', error);
      return { success: false, error: 'Failed to bind TOTP' };
    }
  },

  /**
   * Unbind TOTP
   */
  unbindTotp: async (): Promise<ApiResponse<void>> => {
    try {
      const response = await fetch('/api/remote/totp/unbind', {
        method: 'POST',
      });
      return await response.json();
    } catch (error) {
      console.error('[RemoteAccessApi] unbindTotp error:', error);
      return { success: false, error: 'Failed to unbind TOTP' };
    }
  },

  /**
   * Subscribe to status changes
   */
  onStatusChange: (callback: StatusChangeCallback): (() => void) => {
    statusChangeCallbacks.push(callback);
    setupEventSource();

    return () => {
      statusChangeCallbacks = statusChangeCallbacks.filter(
        (cb) => cb !== callback
      );
      if (statusChangeCallbacks.length === 0 && eventSource) {
        eventSource.close();
        eventSource = null;
      }
    };
  },
};
