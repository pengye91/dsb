// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import axios, { AxiosInstance } from 'axios';
import type {
  Sandbox,
  SandboxConfig,
  ContainerStats,
  ImageSummary,
  ImageDetails,
  PullProgressEvent,
  ErrorResponse,
  ActivityResponse,
  FrontendConfig,
  SandboxProgressEvent,
} from './types';

export class DSBApiClient {
  private client: AxiosInstance;
  private apiKey: string;
  private adminApiKey: string;
  private baseUrl: string;
  private basePath: string;
  private isDev: boolean;

  constructor(baseUrl: string = '/api', apiKey: string = '') {
    // In development, use relative path '/api' to leverage Vite proxy
    // In production, use the configured API URL
    this.isDev = import.meta.env.DEV;
    this.basePath = import.meta.env.VITE_BASE_PATH || '';
    this.baseUrl = this.isDev ? "/api" : this.basePath;

    console.log('[DSBApiClient] Initializing with config:', {
      isDev: this.isDev,
      baseUrl: this.baseUrl,
      hasApiKey: !!apiKey,
    });

    this.client = axios.create({
      baseURL: this.baseUrl,
      headers: {
        'Content-Type': 'application/json',
        ...(apiKey && { 'X-API-Key': apiKey }),
      },
    });
    this.apiKey = apiKey;
    this.adminApiKey = '';
  }

  setApiKey(apiKey: string) {
    console.log('[DSBApiClient] Setting user API key:', apiKey ? '***' : '(empty)');
    this.apiKey = apiKey;
    this._updateApiKeyHeader();
  }

  setAdminApiKey(adminApiKey: string) {
    console.log('[DSBApiClient] Setting admin API key:', adminApiKey ? '***' : '(empty)');
    this.adminApiKey = adminApiKey;
    this._updateApiKeyHeader();
  }

  private _updateApiKeyHeader() {
    // Prefer admin API key if available (it has all permissions)
    // Otherwise, fall back to user API key
    const keyToUse = this.adminApiKey || this.apiKey;
    console.log('[DSBApiClient] Using API key:', keyToUse ? '***' : '(empty)', '(admin:', !!this.adminApiKey, ', user:', !!this.apiKey, ')');
    this.client.defaults.headers['X-API-Key'] = keyToUse;
  }

  getApiKey(): string {
    return this.adminApiKey || this.apiKey;
  }

  // Sandbox operations
  async listSandboxes(filters?: {
    include_deleted?: boolean;
    state?: string;
    image?: string;
    created_after?: string;
    created_before?: string;
    page?: number;
    per_page?: number;
  }): Promise<Sandbox[] | { data: Sandbox[]; pagination: any }> {
    const params: Record<string, string | number> = {};

    if (filters?.include_deleted) {
      params.include_deleted = 'true';
    }
    if (filters?.state) {
      params.state = filters.state;
    }
    if (filters?.image) {
      params.image = filters.image;
    }
    if (filters?.created_after) {
      params.created_after = filters.created_after;
    }
    if (filters?.created_before) {
      params.created_before = filters.created_before;
    }
    if (filters?.page) {
      params.page = filters.page;
    }
    if (filters?.per_page) {
      params.per_page = filters.per_page;
    }

    const response = await this.client.get('/sandboxes', { params });
    return response.data;
  }

  async getConfig(): Promise<FrontendConfig> {
    const response = await this.client.get('/config');
    return response.data;
  }

  async getSandbox(id: string, includeDeleted: boolean = false): Promise<Sandbox> {
    const response = await this.client.get(`/sandboxes/${id}`, {
      params: { include_deleted: includeDeleted }
    });
    return response.data;
  }

  async restoreSandbox(id: string): Promise<Sandbox> {
    const response = await this.client.post(`/sandboxes/${id}/restore`);
    return response.data;
  }

  async getSandboxActivities(
    sandboxId: string,
    limit: number = 50
  ): Promise<ActivityResponse[]> {
    const response = await this.client.get(
      `/sandboxes/${sandboxId}/activities`,
      { params: { limit } }
    );
    return response.data;
  }

  async createSandbox(config: SandboxConfig): Promise<Sandbox> {
    const response = await this.client.post('/sandboxes', config);
    return response.data;
  }

  // SSE streaming for sandbox creation progress
  async streamSandboxCreation(
    config: SandboxConfig,
    onProgress: (event: SandboxProgressEvent) => void,
    onComplete: (sandboxId: string) => void,
    onError: (error: Error) => void
  ): Promise<void> {
    const url = `${this.basePath}/sandboxes/create-stream`;
    const keyToUse = this.adminApiKey || this.apiKey;

    try {
      const response = await fetch(url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(keyToUse ? { 'X-API-Key': keyToUse } : {}),
        },
        body: JSON.stringify(config),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`Failed to create sandbox: ${response.status} ${errorText}`);
      }

      const reader = response.body?.getReader();
      if (!reader) throw new Error('Response body is not readable');

      const decoder = new TextDecoder();
      let buffer = '';

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split('\n');

        // Keep the last partial line in the buffer
        buffer = lines.pop() || '';

        for (const line of lines) {
          if (line.startsWith('data: ')) {
            const data = line.substring(6).trim();
            if (data === 'keepalive') continue;

            try {
              const event = JSON.parse(data) as SandboxProgressEvent;
              onProgress(event);

              if (event.type === 'ready') {
                onComplete(event.sandbox_id);
              } else if (event.type === 'error') {
                throw new Error(event.message || 'Error creating sandbox');
              }
            } catch (e) {
              // Ignore parse errors for malformed data
              if (e instanceof Error && e.message.includes('Error creating sandbox')) {
                throw e;
              }
            }
          }
        }
      }
    } catch (err: any) {
      onError(err);
    }
  }

  async deleteSandbox(id: string): Promise<void> {
    await this.client.delete(`/sandboxes/${id}`);
  }

  async stopSandbox(id: string): Promise<Sandbox> {
    const response = await this.client.post(`/sandboxes/${id}/stop`);
    return response.data;
  }

  async execSandbox(
    id: string,
    command: string[],
    stdin?: string
  ): Promise<{ exit_code: number; stdout: string; stderr: string }> {
    const response = await this.client.post(`/sandboxes/${id}/exec`, {
      command,
      stdin,
    });
    return response.data;
  }

  // Image operations
  async listImages(): Promise<ImageSummary[]> {
    const response = await this.client.get('/images');
    return response.data;
  }

  async pullImage(image: string, tag?: string): Promise<void> {
    await this.client.post('/images/pull', {
      image,
      tag: tag || 'latest',
    });
  }

  async inspectImage(id: string): Promise<ImageDetails> {
    const response = await this.client.get(`/images/${id}`);
    return response.data;
  }

  async deleteImage(id: string): Promise<void> {
    await this.client.delete(`/images/${id}`);
  }

  // SSE for stats
  streamSandboxStats(
    sandboxId: string,
    onStats: (stats: ContainerStats) => void,
    onError?: (error: Event) => void
  ): () => void {
    // EventSource doesn't support headers, so pass API key via query parameter
    // Use relative path which works with both Vite proxy (dev) and same-origin deployment (prod)
    let url = `${this.basePath}/sandboxes/${sandboxId}/stats-stream`;
    const keyToUse = this.adminApiKey || this.apiKey;
    if (keyToUse) {
      url += `?api_key=${encodeURIComponent(keyToUse)}`;
    }

    const eventSource = new EventSource(url);

    eventSource.onmessage = (event) => {
      try {
        const stats = JSON.parse(event.data) as ContainerStats;
        onStats(stats);
      } catch (e) {
        console.error('Failed to parse stats:', e);
      }
    };

    eventSource.onerror = (error) => {
      onError?.(error);
    };

    return () => eventSource.close();
  }

  // SSE for image pull progress
  streamPullProgress(
    image: string,
    tag: string | undefined,
    onProgress: (event: PullProgressEvent) => void,
    onError?: (error: Event) => void
  ): () => void {
    // EventSource doesn't support headers, so pass API key via query parameter
    // Use relative path which works with both Vite proxy (dev) and same-origin deployment (prod)
    const params = new URLSearchParams({
      image,
      tag: tag || 'latest',
    });
    const keyToUse = this.adminApiKey || this.apiKey;
    if (keyToUse) {
      params.set('api_key', keyToUse);
    }
    const url = `${this.basePath}/images/pull-stream?${params.toString()}`;

    const eventSource = new EventSource(url);

    eventSource.onmessage = (event) => {
      try {
        const progress = JSON.parse(event.data) as PullProgressEvent;
        onProgress(progress);
      } catch (e) {
        console.error('Failed to parse progress:', e);
      }
    };

    eventSource.onerror = (error) => {
      onError?.(error);
    };

    return () => eventSource.close();
  }

  // WebSocket for terminal
  connectTerminal(sandboxId: string): WebSocket {
    // Convert current page protocol to ws:// or wss:// and use relative path
    const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    let wsUrl = `${wsProtocol}//${window.location.host}${this.basePath}/terminal/${sandboxId}`;

    const keyToUse = this.adminApiKey || this.apiKey;
    if (keyToUse) {
      wsUrl += `?api_key=${encodeURIComponent(keyToUse)}`;
    }

    return new WebSocket(wsUrl);
  }
}

// Global API client instance
export const apiClient = new DSBApiClient();
