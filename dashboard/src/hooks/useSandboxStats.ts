// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { useState, useEffect, useRef } from 'react';
import { apiClient } from '../api/client';
import type { ContainerStats } from '../api/types';

export function useSandboxStats(sandboxId: string | null, enabled = true) {
  const [stats, setStats] = useState<ContainerStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (!sandboxId || !enabled) return;

    setError(null);

    // Start SSE stream
    cleanupRef.current = apiClient.streamSandboxStats(
      sandboxId,
      (newStats) => {
        setStats(newStats);
        setError(null);
      },
      (err) => {
        console.error('SSE error:', err);
        setError('Connection lost');
      }
    );

    return () => {
      cleanupRef.current?.();
    };
  }, [sandboxId, enabled]);

  return { stats, error };
}
