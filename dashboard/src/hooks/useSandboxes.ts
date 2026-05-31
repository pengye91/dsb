// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { useState, useEffect, useCallback, useRef, useMemo } from 'react';
import { apiClient } from '../api/client';
import type { Sandbox, SandboxConfig, SandboxState, PaginationMeta, SandboxProgressEvent } from '../api/types';

interface SandboxFilters {
  include_deleted?: boolean;
  state?: SandboxState;
  image?: string;
  created_after?: string;
  created_before?: string;
  page?: number;
  per_page?: number;
}

interface SandboxesResponse {
  data: Sandbox[];
  pagination: PaginationMeta;
}

export function useSandboxes(filters?: SandboxFilters) {
  const [sandboxes, setSandboxes] = useState<Sandbox[]>([]);
  const [pagination, setPagination] = useState<PaginationMeta>({
    page: 1,
    per_page: 50,
    total: 0,
    total_pages: 0,
    has_next: false,
    has_prev: false,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Memoize filters to prevent infinite loops when parent creates new objects
  const stableFilters = useMemo(() => filters, [
    filters?.include_deleted,
    filters?.state,
    filters?.image,
    filters?.created_after,
    filters?.created_before,
    filters?.page,
    filters?.per_page,
  ]);

  const loadSandboxes = useCallback(async (newFilters?: SandboxFilters) => {
    try {
      setLoading(true);
      setError(null);
      const response = await apiClient.listSandboxes(newFilters || stableFilters) as SandboxesResponse;
      setSandboxes(response.data || response);
      setPagination(response.pagination || {
        page: 1,
        per_page: Array.isArray(response) ? response.length : 50,
        total: Array.isArray(response) ? response.length : 0,
        total_pages: 1,
        has_next: false,
        has_prev: false,
      });
    } catch (err: any) {
      setError(err.message || 'Failed to load sandboxes');
    } finally {
      setLoading(false);
    }
  }, [stableFilters]); // Depend on stableFilters

  const createSandbox = useCallback(async (config: SandboxConfig) => {
    try {
      const sandbox = await apiClient.createSandbox(config);
      setSandboxes((prev) => [...prev, sandbox]);
      return sandbox;
    } catch (err: any) {
      setError(err.message || 'Failed to create sandbox');
      throw err;
    }
  }, []);

  const createSandboxStream = useCallback(
    (
      config: SandboxConfig,
      onProgress: (event: SandboxProgressEvent) => void,
      onComplete: (sandboxId: string) => void,
      onError: (error: Error) => void
    ) => {
      return apiClient.streamSandboxCreation(config, onProgress, onComplete, onError);
    },
    []
  );

  const deleteSandbox = useCallback(async (id: string) => {
    try {
      await apiClient.deleteSandbox(id);
      // With soft delete, we reload to show the updated state
      await loadSandboxes();
    } catch (err: any) {
      setError(err.message || 'Failed to delete sandbox');
      throw err;
    }
  }, [loadSandboxes]);

  const stopSandbox = useCallback(async (id: string) => {
    try {
      const updated = await apiClient.stopSandbox(id);
      setSandboxes((prev) =>
        prev.map((s) => (s.id === id ? updated : s))
      );
      return updated;
    } catch (err: any) {
      setError(err.message || 'Failed to stop sandbox');
      throw err;
    }
  }, []);

  const restoreSandbox = useCallback(async (id: string) => {
    try {
      const updated = await apiClient.restoreSandbox(id);
      setSandboxes((prev) =>
        prev.map((s) => (s.id === id ? updated : s))
      );
      return updated;
    } catch (err: any) {
      setError(err.message || 'Failed to restore sandbox');
      throw err;
    }
  }, []);

  const nextPage = useCallback(() => {
    if (pagination.has_next) {
      loadSandboxes({ ...stableFilters, page: (pagination.page || 1) + 1 });
    }
  }, [pagination.has_next, pagination.page, loadSandboxes, stableFilters]);

  const prevPage = useCallback(() => {
    if (pagination.has_prev) {
      loadSandboxes({ ...stableFilters, page: (pagination.page || 1) - 1 });
    }
  }, [pagination.has_prev, pagination.page, loadSandboxes, stableFilters]);

  const goToPage = useCallback((page: number) => {
    loadSandboxes({ ...stableFilters, page });
  }, [loadSandboxes, stableFilters]);

  // Reload sandboxes when stableFilters changes
  useEffect(() => {
    loadSandboxes();
  }, [loadSandboxes]);

  return {
    sandboxes,
    pagination,
    loading,
    error,
    refresh: loadSandboxes,
    createSandbox,
    createSandboxStream,
    deleteSandbox,
    stopSandbox,
    restoreSandbox,
    nextPage,
    prevPage,
    goToPage,
  };
}
