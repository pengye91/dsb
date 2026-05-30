// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { useEffect, useRef, useState } from 'react';
import { apiClient } from '../api/client';

export interface TerminalMessage {
  type: 'input' | 'output' | 'resize' | 'error' | 'end';
  data: any;
}

export function useTerminal(sandboxId: string | null) {
  const [isConnected, setIsConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const wsRef = useRef<WebSocket | null>(null);

  const connect = () => {
    if (!sandboxId) return;

    try {
      const ws = apiClient.connectTerminal(sandboxId);
      wsRef.current = ws;

      ws.onopen = () => {
        setIsConnected(true);
        setError(null);
      };

      ws.onerror = (event) => {
        console.error('WebSocket error:', event);
        setError('Connection error');
        setIsConnected(false);
      };

      ws.onclose = () => {
        setIsConnected(false);
      };
    } catch (err) {
      setError('Failed to connect');
      setIsConnected(false);
    }
  };

  const disconnect = () => {
    wsRef.current?.close();
    wsRef.current = null;
    setIsConnected(false);
  };

  const send = (message: TerminalMessage) => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      wsRef.current.send(JSON.stringify(message));
    }
  };

  useEffect(() => {
    if (sandboxId) {
      connect();
    }
    return () => disconnect();
  }, [sandboxId]);

  return {
    isConnected,
    error,
    send,
    connect,
    disconnect,
  };
}
