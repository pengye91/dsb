// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
/**
 * VNC authentication helpers.
 *
 * The viewer owns session-token creation so both the embedded tab and the
 * standalone page can authenticate without depending on another tab's storage.
 */

const VNC_SESSION_TOKEN_STORAGE_PREFIX = 'vnc_token_';
const VNC_SESSION_TOKEN_TTL_SECS = 300;

interface SessionTokenResponse {
  token: string;
}

interface ResolveVncWebSocketUrlOptions {
  /** Force minting a fresh token instead of reusing the cached one. */
  forceFreshToken?: boolean;
  /** URL search string used for backward-compatible api_key fallback. */
  search?: string;
}

/**
 * Build the standalone VNC viewer URL for a sandbox.
 *
 * @param sandboxId - Sandbox ID to open
 * @returns Absolute dashboard URL for the standalone VNC viewer
 */
export function getStandaloneVncViewerUrl(sandboxId: string): string {
  const basePath = import.meta.env.VITE_BASE_PATH || '';
  return `${window.location.origin}${basePath}/vnc-viewer/${sandboxId}`;
}

/**
 * Resolve the WebSocket URL for a VNC connection, preferring a short-lived
 * session token and falling back to legacy API-key auth only if needed.
 *
 * @param sandboxId - Sandbox ID to connect to
 * @param options - Resolution options for retries and backward compatibility
 * @returns Authenticated WebSocket URL
 */
export async function resolveVncWebSocketUrl(
  sandboxId: string,
  options: ResolveVncWebSocketUrlOptions = {},
): Promise<string> {
  const { forceFreshToken = false, search = window.location.search } = options;
  const baseUrl = getVncBaseUrl(sandboxId);

  if (forceFreshToken) {
    sessionStorage.removeItem(getSessionStorageKey(sandboxId));
  }

  const cachedToken = sessionStorage.getItem(getSessionStorageKey(sandboxId));
  if (cachedToken) {
    return `${baseUrl}?token=${encodeURIComponent(cachedToken)}`;
  }

  const apiKeys = getApiKeyCandidates(search);
  for (const apiKey of apiKeys) {
    const token = await createVncSessionToken(sandboxId, apiKey);
    if (token) {
      return `${baseUrl}?token=${encodeURIComponent(token)}`;
    }
  }

  if (apiKeys.length > 0) {
    return `${baseUrl}?api_key=${encodeURIComponent(apiKeys[0])}`;
  }

  return baseUrl;
}

function getSessionStorageKey(sandboxId: string): string {
  return `${VNC_SESSION_TOKEN_STORAGE_PREFIX}${sandboxId}`;
}

function getVncBaseUrl(sandboxId: string): string {
  const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const wsHost = window.location.host;
  const basePath = import.meta.env.VITE_BASE_PATH || '';
  return `${wsProtocol}//${wsHost}${basePath}/vnc/${sandboxId}`;
}

function getApiKeyCandidates(search: string): string[] {
  const urlParams = new URLSearchParams(search);
  const urlApiKey = urlParams.get('api_key')?.trim() || '';
  const adminApiKey = localStorage.getItem('dsb_admin_api_key')?.trim() || '';
  const userApiKey = localStorage.getItem('dsb_api_key')?.trim() || '';

  return Array.from(new Set([urlApiKey, adminApiKey, userApiKey].filter(Boolean)));
}

async function createVncSessionToken(sandboxId: string, apiKey: string): Promise<string | null> {
  const basePath = import.meta.env.VITE_BASE_PATH || '';

  try {
    const response = await fetch(`${basePath}/session-tokens`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-API-Key': apiKey,
      },
      body: JSON.stringify({
        sandbox_id: sandboxId,
        service: 'vnc',
        ttl_secs: VNC_SESSION_TOKEN_TTL_SECS,
      }),
    });

    if (!response.ok) {
      console.warn('[VNC] Failed to create session token', {
        sandboxId,
        status: response.status,
      });
      return null;
    }

    const data = (await response.json()) as SessionTokenResponse;
    if (!data.token) {
      console.warn('[VNC] Session token response missing token', { sandboxId });
      return null;
    }

    sessionStorage.setItem(getSessionStorageKey(sandboxId), data.token);
    return data.token;
  } catch (error) {
    console.error('[VNC] Failed to create session token', { sandboxId, error });
    return null;
  }
}
