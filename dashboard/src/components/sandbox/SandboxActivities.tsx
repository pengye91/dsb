// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import {
  Box,
  VStack,
  HStack,
  Text,
  Badge,
  Accordion,
  AccordionItem,
  AccordionButton,
  AccordionPanel,
  AccordionIcon,
  useColorModeValue,
  Spinner,
  Alert,
  AlertIcon,
} from '@chakra-ui/react';
import { formatRelativeTime } from '../../utils/formatters';

interface ActivityResponse {
  id: string;
  sandbox_id: string;
  activity_type: string;
  timestamp: string;
  details: Record<string, any>;
}

interface SandboxActivitiesProps {
  activities: ActivityResponse[];
  loading?: boolean;
}

// Activity type configuration with labels and colors
const ACTIVITY_TYPE_CONFIG: Record<string, { label: string; colorScheme: string }> = {
  create: { label: 'Created', colorScheme: 'green' },
  delete: { label: 'Deleted', colorScheme: 'red' },
  exec: { label: 'Command Executed', colorScheme: 'blue' },
  stats: { label: 'Stats Queried', colorScheme: 'purple' },
  stop: { label: 'Stopped', colorScheme: 'yellow' },
  cleanup: { label: 'Cleanup', colorScheme: 'orange' },
  info: { label: 'Info Query', colorScheme: 'gray' },
  containerActivity: { label: 'Container Activity', colorScheme: 'teal' },
};

/**
 * SandboxActivities component displays a timeline of sandbox activities.
 *
 * Shows activities in chronological order with:
 * - Color-coded activity type badges
 * - Relative timestamps (e.g., "2 minutes ago")
 * - Expandable details for each activity
 * - Empty state when no activities exist
 */
export const SandboxActivities: React.FC<SandboxActivitiesProps> = ({
  activities,
  loading = false,
}) => {
  const bgColor = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');

  if (loading) {
    return (
      <Box p={8} textAlign="center">
        <Spinner size="lg" />
        <Text mt={4} color="gray.500">
          Loading activities...
        </Text>
      </Box>
    );
  }

  if (activities.length === 0) {
    return (
      <Alert status="info">
        <AlertIcon />
        <Box>
          <Text fontWeight="bold">No activities yet</Text>
          <Text fontSize="sm" color="gray.600">
            Activities will appear here when you interact with this sandbox.
          </Text>
        </Box>
      </Alert>
    );
  }

  return (
    <VStack align="stretch" spacing={0}>
      <Text fontSize="sm" color="gray.500" mb={4}>
        Showing {activities.length} most recent activities
      </Text>

      <VStack align="stretch" spacing={0}>
        {activities.map((activity, index) => {
          const config = ACTIVITY_TYPE_CONFIG[activity.activity_type] || {
            label: activity.activity_type,
            colorScheme: 'gray',
          };

          return (
            <Box
              key={activity.id}
              bg={bgColor}
              p={4}
              borderWidth="1px"
              borderColor={borderColor}
              borderRadius="md"
              _first={{ borderTopRadius: 'lg' }}
              _last={{ borderBottomRadius: 'lg' }}
              _notLast={{ borderBottomWidth: 0 }}
            >
              <Accordion allowToggle defaultIndex={[]}>
                <AccordionItem border="none">
                  <AccordionButton
                    px={0}
                    py={0}
                    _hover={{ bg: 'transparent' }}
                    _focus={{ boxShadow: 'none' }}
                  >
                    <HStack flex="1" justify="space-between" align="center">
                      <HStack spacing={3} align="center">
                        <Badge colorScheme={config.colorScheme} fontSize="sm" px={3} py={1}>
                          {config.label}
                        </Badge>
                        <Text fontSize="sm" color="gray.500">
                          {formatRelativeTime(activity.timestamp)}
                        </Text>
                      </HStack>
                      <AccordionIcon />
                    </HStack>
                  </AccordionButton>
                  <AccordionPanel px={0} pt={4} pb={0}>
                    <Box
                      bg={useColorModeValue('gray.50', 'gray.900')}
                      p={4}
                      borderRadius="md"
                      borderWidth="1px"
                      borderColor={borderColor}
                    >
                      <VStack align="stretch" spacing={3}>
                        <Box>
                          <Text fontSize="xs" fontWeight="bold" color="gray.500" mb={2}>
                            ACTIVITY ID
                          </Text>
                          <Text fontSize="sm" fontFamily="mono">
                            {activity.id.slice(0, 8)}
                          </Text>
                        </Box>

                        <Box>
                          <Text fontSize="xs" fontWeight="bold" color="gray.500" mb={2}>
                            TIMESTAMP
                          </Text>
                          <Text fontSize="sm">
                            {new Date(activity.timestamp).toLocaleString()}
                          </Text>
                        </Box>

                        {Object.keys(activity.details).length > 0 && (
                          <Box>
                            <Text fontSize="xs" fontWeight="bold" color="gray.500" mb={2}>
                              DETAILS
                            </Text>
                            <VStack align="stretch" spacing={2}>
                              {Object.entries(activity.details).map(([key, value]) => (
                                <HStack key={key} justify="space-between" align="start">
                                  <Text
                                    fontSize="sm"
                                    color="gray.500"
                                    minW="120px"
                                    wordBreak="break-word"
                                  >
                                    {key}
                                  </Text>
                                  <Text
                                    fontSize="sm"
                                    textAlign="right"
                                    flex="1"
                                    wordBreak="break-all"
                                  >
                                    {formatDetailValue(value)}
                                  </Text>
                                </HStack>
                              ))}
                            </VStack>
                          </Box>
                        )}
                      </VStack>
                    </Box>
                  </AccordionPanel>
                </AccordionItem>
              </Accordion>
            </Box>
          );
        })}
      </VStack>
    </VStack>
  );
};

/**
 * Format detail values for display.
 * Handles arrays, objects, and primitives.
 */
function formatDetailValue(value: any): string {
  if (value === null || value === undefined) {
    return 'N/A';
  }

  if (Array.isArray(value)) {
    if (value.length === 0) return '[]';
    return `[${value.slice(0, 5).join(', ')}${value.length > 5 ? '...' : ''}]`;
  }

  if (typeof value === 'object') {
    const entries = Object.entries(value);
    if (entries.length === 0) return '{}';
    return JSON.stringify(value, null, 2);
  }

  if (typeof value === 'boolean') {
    return value ? 'Yes' : 'No';
  }

  return String(value);
}
