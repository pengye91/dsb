// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import {
  Heading,
  SimpleGrid,
  Box,
  Text,
  HStack,
  Button,
  VStack,
  useColorModeValue,
  Spinner,
} from '@chakra-ui/react';
import { Plus, Activity, Container, Server } from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import { useSandboxes } from '../hooks/useSandboxes';

export default function Dashboard() {
  const { sandboxes, loading, error } = useSandboxes();
  const bgColor = useColorModeValue('white', 'gray.800');

  const runningCount = sandboxes.filter((s) => s.state === 'running').length;
  const stoppedCount = sandboxes.filter((s) => s.state === 'stopped').length;
  const errorCount = sandboxes.filter((s) => s.state === 'error').length;

  if (loading) {
    return (
      <Box p={8} textAlign="center">
        <Spinner size="xl" />
        <Text mt={4}>Loading dashboard...</Text>
      </Box>
    );
  }

  if (error) {
    return (
      <Box p={4} bg="red.50" borderRadius="md">
        <Text color="red.500">{error}</Text>
      </Box>
    );
  }

  return (
    <VStack align="stretch" spacing={6}>
      {/* Header */}
      <HStack justify="space-between">
        <Heading size="lg">Dashboard</Heading>
        <Button
          as={Link}
          to="/sandboxes/create"
          colorScheme="blue"
          leftIcon={<Plus size={18} />}
        >
          Create Sandbox
        </Button>
      </HStack>

      {/* Stats Overview */}
      <SimpleGrid columns={{ base: 1, md: 4 }} spacing={4}>
        <StatBox
          label="Total Sandboxes"
          value={sandboxes.length}
          icon={<Container size={20} />}
          color="blue"
        />
        <StatBox
          label="Running"
          value={runningCount}
          icon={<Activity size={20} />}
          color="green"
        />
        <StatBox
          label="Stopped"
          value={stoppedCount}
          icon={<Server size={20} />}
          color="gray"
        />
        <StatBox
          label="Errors"
          value={errorCount}
          icon={<Activity size={20} />}
          color="red"
        />
      </SimpleGrid>

      {/* Recent Sandboxes */}
      <Box>
        <Heading size="md" mb={4}>
          Recent Sandboxes
        </Heading>
        {sandboxes.length === 0 ? (
          <Box p={8} textAlign="center" bg={bgColor} borderRadius="lg">
            <Text color="gray.500">No sandboxes yet</Text>
            <Button
              as={Link}
              to="/sandboxes/create"
              mt={4}
              colorScheme="blue"
              size="sm"
            >
              Create your first sandbox
            </Button>
          </Box>
        ) : (
          <SimpleGrid columns={{ base: 1, md: 2, lg: 3 }} spacing={4}>
            {sandboxes.slice(0, 6).map((sandbox) => (
              <SandboxSummaryCard key={sandbox.id} sandbox={sandbox} />
            ))}
          </SimpleGrid>
        )}
      </Box>
    </VStack>
  );
}

interface StatBoxProps {
  label: string;
  value: number;
  icon: React.ReactNode;
  color: string;
}

function StatBox({ label, value, icon, color }: StatBoxProps) {
  const bg = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');

  return (
    <Box
      bg={bg}
      p={4}
      borderRadius="lg"
      borderWidth="1px"
      borderColor={borderColor}
    >
      <HStack justify="space-between">
        <Box color={`${color}.500`}>{icon}</Box>
        <Text fontSize="2xl" fontWeight="bold">
          {value}
        </Text>
      </HStack>
      <Text fontSize="sm" color="gray.500" mt={2}>
        {label}
      </Text>
    </Box>
  );
}

interface SandboxSummaryCardProps {
  sandbox: any;
}

function SandboxSummaryCard({ sandbox }: SandboxSummaryCardProps) {
  const bg = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');
  const navigate = useNavigate();

  const stateColors: Record<string, string> = {
    running: 'green',
    stopped: 'gray',
    error: 'red',
    creating: 'blue',
    starting: 'yellow',
  };

  return (
    <Box
      bg={bg}
      p={4}
      borderRadius="lg"
      borderWidth="1px"
      borderColor={borderColor}
      cursor="pointer"
      onClick={() => navigate(`/sandboxes/${sandbox.id}`)}
      _hover={{
        shadow: 'md',
        transform: 'translateY(-2px)',
      }}
      transition="all 0.2s"
    >
      <VStack align="stretch" spacing={3}>
        <HStack justify="space-between">
          <Text fontWeight="bold" noOfLines={1}>
            {sandbox.config.name || sandbox.id.slice(0, 8)}
          </Text>
          <Text
            fontSize="xs"
            px={2}
            py={1}
            borderRadius="md"
            bg={`${stateColors[sandbox.state]}.100`}
            color={`${stateColors[sandbox.state]}.700`}
          >
            {sandbox.state}
          </Text>
        </HStack>

        <Text fontSize="sm" color="gray.500" noOfLines={1}>
          {sandbox.config.image}
        </Text>
      </VStack>
    </Box>
  );
}
