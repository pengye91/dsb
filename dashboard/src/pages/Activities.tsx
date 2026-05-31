// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import { Heading, Text, Box, VStack } from '@chakra-ui/react';

export default function Activities() {
  return (
    <VStack align="stretch" spacing={6}>
      <Heading size="lg">Activity Log</Heading>
      <Box p={8} textAlign="center" bg="white" borderRadius="lg">
        <Text color="gray.500">Activity tracking coming soon...</Text>
      </Box>
    </VStack>
  );
}
