// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
/**
 * Open VNC Button Component
 *
 * Button component that opens the standalone VNC viewer in a new browser tab.
 */

import React from 'react';
import { Button, IconButton } from '@chakra-ui/react';
import { ExternalLink } from 'lucide-react';
import type { Sandbox } from '../../api/types';
import { getStandaloneVncViewerUrl } from './auth';

export interface OpenVNCButtonProps {
  /** Sandbox object */
  sandbox: Sandbox;
  /** Button variant */
  variant?: 'solid' | 'ghost' | 'outline';
  /** Button size */
  size?: 'sm' | 'md' | 'lg';
  /** Disable button */
  disabled?: boolean;
  /** Show as icon button */
  isIcon?: boolean;
  /** Custom label */
  label?: string;
}

/**
 * Button that opens the standalone VNC viewer in a new tab
 *
 * @example
 * ```tsx
 * <OpenVNCButton sandbox={sandbox} />
 * <OpenVNCButton sandbox={sandbox} variant="ghost" size="sm" isIcon />
 * ```
 */
export function OpenVNCButton({
  sandbox,
  variant = 'solid',
  size = 'sm',
  disabled = false,
  isIcon = false,
  label = 'Open in New Tab',
}: OpenVNCButtonProps) {
  const handleClick = () => {
    window.open(getStandaloneVncViewerUrl(sandbox.id), '_blank', 'noopener,noreferrer');
  };

  // Only enable button when sandbox is running
  const isDisabled = disabled || sandbox.state !== 'running';

  if (isIcon) {
    return (
      <IconButton
        aria-label="Open VNC in new tab"
        icon={<ExternalLink size={16} />}
        onClick={handleClick}
        isDisabled={isDisabled}
        variant={variant}
        size={size}
        title={isDisabled ? 'Sandbox must be running' : label}
      />
    );
  }

  return (
    <Button
      leftIcon={<ExternalLink size={size === 'lg' ? 20 : size === 'md' ? 18 : 16} />}
      onClick={handleClick}
      isDisabled={isDisabled}
      variant={variant}
      size={size}
      colorScheme="blue"
      title={isDisabled ? 'Sandbox must be running' : undefined}
    >
      {label}
    </Button>
  );
}
