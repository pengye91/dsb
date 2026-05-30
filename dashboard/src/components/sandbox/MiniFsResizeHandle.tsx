// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import { Box, useColorModeValue } from '@chakra-ui/react';

interface MiniFsResizeHandleProps {
  // No props needed - this is a purely visual component
}

/**
 * Visual drag handle for resizable MiniFs panels
 * Displays visual indicators for the resize handle area
 * Actual drag behavior is handled by react-resizable-panels PanelResizeHandle
 */
export function MiniFsResizeHandle({}: MiniFsResizeHandleProps = {}) {
  const handleBg = useColorModeValue('gray.100', 'gray.700');
  const handleHoverBg = useColorModeValue('gray.200', 'gray.600');
  const handleBorder = useColorModeValue('gray.300', 'gray.600');
  const [isDragging, setIsDragging] = React.useState(false);

  React.useEffect(() => {
    const handleMouseDown = () => {
      setIsDragging(true);
    };

    const handleMouseUp = () => {
      setIsDragging(false);
    };

    // Listen for drag state changes from PanelResizeHandle
    document.addEventListener('mousedown', handleMouseDown);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousedown', handleMouseDown);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, []);

  return (
    <Box
      display="flex"
      alignItems="stretch"
      justifyContent="center"
      bg={handleBg}
      borderLeft="1px"
      borderRight="1px"
      borderColor={handleBorder}
      width="8px"
      height="full"
      cursor="col-resize"
      _hover={{
        bg: handleHoverBg,
      }}
      transition="background 0.15s"
      position="relative"
      style={{ pointerEvents: 'auto' }}
    >
      {/* Drag indicator */}
      <Box
        display="flex"
        flexDirection="column"
        gap={1}
        opacity={isDragging ? 1 : 0.5}
        transition="opacity 0.15s"
        width="full"
        height="full"
        alignItems="center"
        justifyContent="center"
        pointerEvents="none"
      >
        {[1, 2, 3].map((i) => (
          <Box
            key={i}
            width="2px"
            height="2px"
            borderRadius="full"
            bg={handleBorder}
          />
        ))}
      </Box>
    </Box>
  );
}
