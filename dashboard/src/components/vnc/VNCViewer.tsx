// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
/**
 * VNC Session Component
 *
 * Reusable VNC viewer component with connection management, auto-retry, and error handling.
 * Extracted from SandboxDetails.tsx VNCComponent for use in multiple contexts.
 */

import React, { useEffect, useRef, useState } from 'react';
import {
  Box,
  VStack,
  HStack,
  Text,
  Spinner,
  Alert,
  AlertIcon,
  Button,
} from '@chakra-ui/react';
import { VncScreen } from 'react-vnc';
import type { Sandbox } from '../../api/types';
import { resolveVncWebSocketUrl } from './auth';

export interface VNCViewerProps {
  /** Sandbox object containing ID and state */
  sandbox: Sandbox;
  /** Optional custom styles */
  style?: React.CSSProperties;
  /** Show connection status indicator */
  showConnectionStatus?: boolean;
  /** Optional resolution override */
  resolution?: string;
  /** Whether to fill parent height (for standalone full-page VNC) */
  fullHeight?: boolean;
}

type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';

export function VNCViewer({
  sandbox,
  style,
  showConnectionStatus = true,
  resolution,
  fullHeight = false,
}: VNCViewerProps) {
  const [connectionState, setConnectionState] = useState<ConnectionState>('disconnected');
  const [error, setError] = useState<string | null>(null);
  const [retryCount, setRetryCount] = useState(0);
  const [isRetrying, setIsRetrying] = useState(false);
  const [vncUrl, setVncUrl] = useState<string | null>(null);
  const vncRef = useRef<any>(null);

  // Set connecting state when component mounts or sandbox changes
  useEffect(() => {
    if (sandbox.state === 'running') {
      setConnectionState('connecting');
      setRetryCount(0);
      setError(null);
      setVncUrl(null);
    }
  }, [sandbox.id, sandbox.state]);

  useEffect(() => {
    if (sandbox.state !== 'running') {
      setVncUrl(null);
      return;
    }

    let cancelled = false;

    const prepareVncSession = async () => {
      try {
        const url = await resolveVncWebSocketUrl(sandbox.id, {
          forceFreshToken: retryCount > 0,
        });

        if (!cancelled) {
          setVncUrl(url);
        }
      } catch (authError) {
        console.error('[VNC] Failed to prepare VNC authentication', authError);
        if (!cancelled) {
          setError('Failed to authorize VNC connection');
          setConnectionState('error');
        }
      }
    };

    void prepareVncSession();

    return () => {
      cancelled = true;
    };
  }, [sandbox.id, sandbox.state, retryCount]);

  // Auto-retry logic with exponential backoff
  useEffect(() => {
    if (connectionState === 'error' && retryCount < 5) {
      const backoffDelay = Math.min(1000 * Math.pow(2, retryCount), 10000); // Max 10 seconds

      const timer = setTimeout(() => {
        setIsRetrying(true);
        setConnectionState('connecting');
        setRetryCount((prev) => prev + 1);

        // Force re-render by updating vncRef key
        setTimeout(() => {
          setIsRetrying(false);
        }, 100);
      }, backoffDelay);

      return () => clearTimeout(timer);
    }
  }, [connectionState, retryCount]);

  const handleDisconnect = (e: any) => {
    console.log('[VNC] Disconnected:', e);
    setConnectionState('error');
    setError('Connection lost. Retrying...');
  };

  const getProgressMessage = () => {
    if (!vncUrl) {
      return retryCount > 0 ? 'Refreshing VNC session...' : 'Authorizing VNC session...';
    }
    if (isRetrying) {
      return `Retrying connection... (Attempt ${retryCount + 1}/5)`;
    }
    if (retryCount > 0) {
      return `Establishing VNC connection... (Attempt ${retryCount + 1}/5)`;
    }
    return 'Establishing VNC connection...';
  };

  if (!sandbox || sandbox.state !== 'running') {
    return (
      <Alert status="info">
        <AlertIcon />
        <VStack align="start" spacing={2}>
          <Text fontWeight="bold">Sandbox is not running</Text>
          <Text fontSize="sm">Start the sandbox to access VNC.</Text>
        </VStack>
      </Alert>
    );
  }

  return (
    <VStack
      align="stretch"
      spacing={4}
      {...(fullHeight && { flex: 1, minH: 0 })}
    >
      {connectionState === 'error' && error && (
        <Alert status="warning">
          <AlertIcon />
          <VStack align="start" spacing={2}>
            <Text fontWeight="bold">Connection Issue</Text>
            <Text fontSize="sm">{error}</Text>
            {retryCount < 5 && (
              <Text fontSize="xs" color="gray.600">
                Auto-retry in progress... ({5 - retryCount} attempts remaining)
              </Text>
            )}
            {retryCount >= 5 && (
              <HStack spacing={2}>
                <Button
                  size="sm"
                  colorScheme="blue"
                  onClick={() => {
                    setRetryCount(0);
                    setConnectionState('connecting');
                  }}
                >
                  Retry Now
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setConnectionState('connecting');
                    setRetryCount(0);
                  }}
                >
                  Cancel
                </Button>
              </HStack>
            )}
          </VStack>
        </Alert>
      )}

      {connectionState === 'connecting' && (
        <Alert status="info">
          <AlertIcon />
          <VStack align="start" spacing={3}>
            <HStack>
              <Spinner size="sm" />
              <Text fontWeight="bold">Connecting to VNC</Text>
            </HStack>
            <Text fontSize="sm">{getProgressMessage()}</Text>
            <Text fontSize="xs" color="gray.600">
              This may take a few seconds if the sandbox was just started...
            </Text>
          </VStack>
        </Alert>
      )}

      <Box
        bg="black"
        borderRadius="md"
        overflow="hidden"
        h={fullHeight ? 'auto' : '600px'}
        flex={fullHeight ? 1 : undefined}
        minH={fullHeight ? 0 : undefined}
        w="100%"
        position="relative"
      >
        {connectionState === 'connecting' && (
          <Box
            position="absolute"
            top="0"
            left="0"
            right="0"
            bottom="0"
            display="flex"
            alignItems="center"
            justifyContent="center"
            flexDirection="column"
            bg="black"
            zIndex={10}
          >
            <Spinner size="xl" thickness="4" color="white" mb={4} />
            <Text color="white" fontSize="sm">
              {getProgressMessage()}
            </Text>
          </Box>
        )}

        {vncUrl && (
          <VncScreen
            ref={vncRef}
            key={`vnc-${sandbox.id}-${sandbox.container_id || 'none'}-${retryCount}-${vncUrl}`}
            url={vncUrl}
            scaleViewport
            resizeSession
            style={{
              width: '100%',
              height: '100%',
              display: connectionState === 'connecting' ? 'none' : 'block',
              backgroundColor: '#000',
              ...style,
            }}
            onConnect={() => {
              console.log('[VNC] onConnect callback fired');
              setConnectionState('connected');
              setError(null);
              setRetryCount(0);
            }}
            onDisconnect={(e: any) => {
              console.log('[VNC] onDisconnect callback:', e);
              handleDisconnect(e);
            }}
            onCredentialsRequired={() => {
              console.log('[VNC] onCredentialsRequired callback fired');
              setError('VNC server requires authentication');
              setConnectionState('error');
            }}
            onSecurityFailure={(e: any) => {
              console.error('[VNC] onSecurityFailure callback:', e);
              setError('VNC security negotiation failed');
              setConnectionState('error');
            }}
          />
        )}
      </Box>

      {showConnectionStatus && (
        <HStack fontSize="sm" color="gray.500">
          <Text>Connection: </Text>
          <Text
            fontWeight="bold"
            color={
              connectionState === 'connected'
                ? 'green.500'
                : connectionState === 'connecting'
                  ? 'blue.500'
                  : connectionState === 'error'
                    ? 'red.500'
                    : 'gray.500'
            }
          >
            {connectionState.charAt(0).toUpperCase() + connectionState.slice(1)}
          </Text>
          <Text>•</Text>
          <Text>Sandbox: {sandbox.id.slice(0, 8)}</Text>
          {resolution && (
            <>
              <Text>•</Text>
              <Text>Resolution: {resolution}</Text>
            </>
          )}
          {connectionState === 'connected' && (
            <>
              <Text>•</Text>
              <Text>Ready</Text>
            </>
          )}
        </HStack>
      )}
    </VStack>
  );
}
