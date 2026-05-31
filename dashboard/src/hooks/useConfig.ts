// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { useState, useEffect } from 'react';
import { apiClient } from '../api/client';
import type { FrontendConfig } from '../api/types';

/**
 * Hook to fetch and manage frontend configuration from the backend.
 *
 * This hook fetches configuration values like:
 * - Default sandbox image
 * - Default timeouts
 * - Authentication requirements
 *
 * The configuration is fetched once on mount and cached.
 *
 * @returns Object with config, loading state, and error state
 *
 * @example
 * ```tsx
 * function MyComponent() {
 *   const { config, loading, error } = useConfig();
 *
 *   if (loading) return <div>Loading...</div>;
 *   if (error) return <div>Error: {error}</div>;
 *
 *   return <div>Default image: {config?.default_sandbox_image}</div>;
 * }
 * ```
 */
export function useConfig() {
  const [config, setConfig] = useState<FrontendConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadConfig() {
      try {
        setLoading(true);
        setError(null);
        const cfg = await apiClient.getConfig();
        if (!cancelled) {
          setConfig(cfg);
        }
      } catch (err: any) {
        console.error('[useConfig] Failed to load config:', err);
        if (!cancelled) {
          setError(err.message || 'Failed to load configuration');
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    loadConfig();

    return () => {
      cancelled = true;
    };
  }, []);

  return { config, loading, error };
}
