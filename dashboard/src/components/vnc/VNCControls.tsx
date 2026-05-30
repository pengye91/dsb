// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
/**
 * VNC Controls Component
 *
 * Control panel for VNC viewer with status, resolution info, and actions.
 */

import React from 'react';
import {
  VStack,
  HStack,
  Text,
  Badge,
  IconButton,
  Tooltip,
  Box,
} from '@chakra-ui/react';
import {
  RotateCcw,
  Maximize,
  Monitor,
} from 'lucide-react';

export interface VNCControlsProps {
  /** Current connection state */
  connectionState: 'connecting' | 'connected' | 'error' | 'disconnected';
  /** VNC resolution (if known) */
  resolution?: string;
  /** Sandbox name (optional) */
  sandboxName?: string;
  /** Callback for reconnect button */
  onReconnect?: () => void;
  /** Callback for fullscreen button */
  onFullscreen?: () => void;
  /** Show compact version */
  compact?: boolean;
}

/**
 * VNC viewer control panel with status and actions
 *
 * @example
 * ```tsx
 * <VNCControls
 *   connectionState="connected"
 *   resolution="2560x1440"
 *   sandboxName="my-sandbox"
 *   onReconnect={() => reconnect()}
 *   onFullscreen={() => toggleFullscreen()}
 * />
 * ```
 */
export function VNCControls({
  connectionState,
  resolution,
  sandboxName,
  onReconnect,
  onFullscreen,
  compact = false,
}: VNCControlsProps) {
  const getStateColor = () => {
    switch (connectionState) {
      case 'connected':
        return 'green';
      case 'connecting':
        return 'blue';
      case 'error':
        return 'red';
      default:
        return 'gray';
    }
  };

  const getStateLabel = () => {
    switch (connectionState) {
      case 'connected':
        return 'Connected';
      case 'connecting':
        return 'Connecting...';
      case 'error':
        return 'Error';
      default:
        return 'Disconnected';
    }
  };

  if (compact) {
    return (
      <HStack spacing={3} px={4} py={2} bg="gray.50" borderRadius="md">
        <HStack spacing={2}>
          <Monitor size={16} />
          <Text fontSize="sm" fontWeight="medium">
            {sandboxName || 'VNC'}
          </Text>
        </HStack>
        <Badge colorScheme={getStateColor()} fontSize="xs">
          {getStateLabel()}
        </Badge>
        {resolution && (
          <Text fontSize="xs" color="gray.600">
            {resolution}
          </Text>
        )}
      </HStack>
    );
  }

  return (
    <VStack align="stretch" spacing={3} p={4} bg="gray.50" borderRadius="md">
      <HStack justify="space-between">
        <HStack spacing={3}>
          <HStack spacing={2}>
            <Monitor size={20} />
            <Text fontWeight="bold">VNC Viewer</Text>
            {sandboxName && (
              <Text fontSize="sm" color="gray.600">
                ({sandboxName})
              </Text>
            )}
          </HStack>
          <Badge colorScheme={getStateColor()} fontSize="sm">
            {getStateLabel()}
          </Badge>
        </HStack>

        <HStack spacing={2}>
          {onReconnect && connectionState !== 'connecting' && (
            <Tooltip label="Reconnect" placement="top">
              <IconButton
                aria-label="Reconnect"
                icon={<RotateCcw size={18} />}
                onClick={onReconnect}
                variant="ghost"
                size="sm"
              />
            </Tooltip>
          )}
          {onFullscreen && connectionState === 'connected' && (
            <Tooltip label="Fullscreen" placement="top">
              <IconButton
                aria-label="Fullscreen"
                icon={<Maximize size={18} />}
                onClick={onFullscreen}
                variant="ghost"
                size="sm"
              />
            </Tooltip>
          )}
        </HStack>
      </HStack>

      {resolution && (
        <HStack fontSize="sm" color="gray.600">
          <Text>Resolution:</Text>
          <Text fontWeight="medium">{resolution}</Text>
        </HStack>
      )}
    </VStack>
  );
}
