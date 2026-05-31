// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import {
  Box,
  Flex,
  Heading,
  Spacer,
  Button,
  HStack,
  Text,
  useColorModeValue,
} from '@chakra-ui/react';
import { Settings, Server } from 'lucide-react';

interface HeaderProps {
  onOpenSettings: () => void;
  apiKey?: string;
}

export function Header({ onOpenSettings, apiKey }: HeaderProps) {
  const bg = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');

  return (
    <Box
      bg={bg}
      borderBottom="1px"
      borderColor={borderColor}
      px={6}
      py={4}
    >
      <Flex align="center" minH="12">
        <HStack spacing={3}>
          <Server size={24} color="#3182ce" />
          <Heading size="md">DSB Dashboard</Heading>
        </HStack>

        <Spacer />

        <HStack spacing={4}>
          {apiKey && (
            <Text fontSize="sm" color="gray.500">
              Authenticated
            </Text>
          )}
          <Button
            leftIcon={<Settings size={16} />}
            variant="ghost"
            size="sm"
            onClick={onOpenSettings}
          >
            Settings
          </Button>
        </HStack>
      </Flex>
    </Box>
  );
}
