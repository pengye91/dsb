// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React, { useState } from 'react';
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
  Badge,
  Checkbox,
  Select,
  IconButton,
} from '@chakra-ui/react';
import { Plus, StopCircle, Trash2, Terminal, ChevronLeft, ChevronRight, Filter, RotateCcw } from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import { useSandboxes } from '../hooks/useSandboxes';
import { formatRelativeTime } from '../utils/formatters';
import type { SandboxState } from '../api/types';

export default function Sandboxes() {
  const [includeDeleted, setIncludeDeleted] = useState(false);
  const [stateFilter, setStateFilter] = useState<string>('');
  const [showFilters, setShowFilters] = useState(false);
  const [restoringId, setRestoringId] = useState<string | null>(null);

  const {
    sandboxes,
    pagination,
    loading,
    error,
    refresh,
    deleteSandbox,
    stopSandbox,
    restoreSandbox,
    nextPage,
    prevPage,
    goToPage,
  } = useSandboxes({
    include_deleted: includeDeleted || undefined,
    state: stateFilter as SandboxState || undefined,
  });

  const bgColor = useColorModeValue('white', 'gray.800');

  const handleStop = async (id: string, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      await stopSandbox(id);
      refresh();
    } catch (err) {
      alert('Failed to stop sandbox');
    }
  };

  const handleDelete = async (id: string, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!confirm('Are you sure you want to delete this sandbox?')) return;

    try {
      await deleteSandbox(id);
    } catch (err) {
      alert('Failed to delete sandbox');
    }
  };

  const handleRestore = async (id: string, e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    setRestoringId(id);
    try {
      await restoreSandbox(id);
      await refresh();
    } catch (err) {
      alert('Failed to restore sandbox');
    } finally {
      setRestoringId(null);
    }
  };

  if (loading) {
    return (
      <Box p={8} textAlign="center">
        <Spinner size="xl" />
        <Text mt={4}>Loading sandboxes...</Text>
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
        <Heading size="lg">Sandboxes</Heading>
        <HStack spacing={3}>
          <Button
            leftIcon={<Filter size={18} />}
            variant="outline"
            onClick={() => setShowFilters(!showFilters)}
          >
            Filters
          </Button>
          <Button
            as={Link}
            to="/sandboxes/create"
            colorScheme="blue"
            leftIcon={<Plus size={18} />}
          >
            Create Sandbox
          </Button>
        </HStack>
      </HStack>

      {/* Filters */}
      {showFilters && (
        <Box p={4} bg={bgColor} borderRadius="lg" borderWidth="1px">
          <VStack align="stretch" spacing={4}>
            <HStack spacing={6}>
              <Checkbox
                isChecked={includeDeleted}
                onChange={(e) => setIncludeDeleted(e.target.checked)}
              >
                Include deleted sandboxes
              </Checkbox>
              <Box>
                <Text fontSize="sm" mb={1}>Filter by state:</Text>
                <Select
                  placeholder="All states"
                  value={stateFilter}
                  onChange={(e) => setStateFilter(e.target.value)}
                  width="200px"
                >
                  <option value="">All states</option>
                  <option value="creating">Creating</option>
                  <option value="created">Created</option>
                  <option value="starting">Starting</option>
                  <option value="running">Running</option>
                  <option value="stopped">Stopped</option>
                  <option value="error">Error</option>
                  <option value="destroying">Destroying</option>
                  <option value="destroyed">Destroyed</option>
                </Select>
              </Box>
            </HStack>
            <Text fontSize="xs" color="gray.500">
              Showing {pagination.total} sandboxes (page {pagination.page} of {pagination.total_pages})
            </Text>
          </VStack>
        </Box>
      )}

      {/* Pagination Controls */}
      {pagination.total_pages > 1 && (
        <HStack justify="center" spacing={4}>
          <IconButton
            icon={<ChevronLeft size={20} />}
            aria-label="Previous page"
            isDisabled={!pagination.has_prev}
            onClick={prevPage}
            variant="ghost"
          />
          <Text>
            Page {pagination.page} of {pagination.total_pages}
          </Text>
          <IconButton
            icon={<ChevronRight size={20} />}
            aria-label="Next page"
            isDisabled={!pagination.has_next}
            onClick={nextPage}
            variant="ghost"
          />
        </HStack>
      )}

      {/* Sandbox Grid */}
      {sandboxes.length === 0 ? (
        <Box p={8} textAlign="center" bg={bgColor} borderRadius="lg">
          <Text color="gray.500">
            {includeDeleted || stateFilter ? 'No sandboxes match your filters' : 'No sandboxes yet'}
          </Text>
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
          {sandboxes.map((sandbox) => (
            <SandboxCard
              key={sandbox.id}
              sandbox={sandbox}
              onStop={(e) => handleStop(sandbox.id, e)}
              onDelete={(e) => handleDelete(sandbox.id, e)}
              onRestore={(e) => handleRestore(sandbox.id, e)}
              isRestoring={restoringId === sandbox.id}
            />
          ))}
        </SimpleGrid>
      )}
    </VStack>
  );
}

interface SandboxCardProps {
  sandbox: any;
  onStop: (e: React.MouseEvent) => void;
  onDelete: (e: React.MouseEvent) => void;
  onRestore: (e: React.MouseEvent) => void;
  isRestoring?: boolean;
}

function SandboxCard({ sandbox, onStop, onDelete, onRestore, isRestoring = false }: SandboxCardProps) {
  const bg = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');
  const navigate = useNavigate();

  const handleClick = () => {
    navigate(`/sandboxes/${sandbox.id}`);
  };

  const stateColors: Record<string, string> = {
    running: 'green',
    stopped: 'gray',
    error: 'red',
    creating: 'blue',
    starting: 'yellow',
    destroying: 'orange',
    destroyed: 'gray',
  };

  const isDeleted = !!sandbox.deleted_at;

  return (
    <Box
      bg={bg}
      p={4}
      borderRadius="lg"
      borderWidth="1px"
      borderColor={borderColor}
      cursor="pointer"
      onClick={handleClick}
      opacity={isDeleted ? 0.7 : 1}
      _hover={{
        shadow: 'md',
        transform: 'translateY(-2px)',
      }}
      transition="all 0.2s"
      position="relative"
    >
      {isDeleted && (
        <Badge
          position="absolute"
          top={2}
          right={2}
          colorScheme="red"
          variant="solid"
        >
          Deleted
        </Badge>
      )}
      <VStack align="stretch" spacing={3}>
        {/* Header */}
        <HStack justify="space-between">
          <Text fontWeight="bold" noOfLines={1}>
            {sandbox.config.name || sandbox.id.slice(0, 8)}
          </Text>
          <Badge colorScheme={stateColors[sandbox.state] || 'gray'}>
            {sandbox.state}
          </Badge>
        </HStack>

        {/* Image */}
        <Text fontSize="sm" color="gray.500" noOfLines={1}>
          {sandbox.config.image}
        </Text>

        {/* Timestamp */}
        <Text fontSize="xs" color="gray.400">
          Created {formatRelativeTime(sandbox.created_at)}
        </Text>

        {/* Deleted info */}
        {isDeleted && (
          <Text fontSize="xs" color="red.500">
            Deleted {formatRelativeTime(sandbox.deleted_at)}
            {sandbox.deleted_by && ` by ${sandbox.deleted_by}`}
          </Text>
        )}

        {/* Actions */}
        <HStack justify="flex-end" spacing={2}>
          {isDeleted && (
            <Button
              size="sm"
              variant="ghost"
              colorScheme="green"
              leftIcon={<RotateCcw size={14} />}
              onClick={(e) => {
                e.stopPropagation();
                onRestore(e);
              }}
              isLoading={isRestoring}
              isDisabled={isRestoring}
            >
              Restore
            </Button>
          )}
          {!isDeleted && sandbox.state === 'running' && (
            <>
              <Button
                size="sm"
                variant="ghost"
                leftIcon={<Terminal size={14} />}
                onClick={(e) => {
                  e.stopPropagation();
                  navigate(`/sandboxes/${sandbox.id}/terminal`);
                }}
              >
                Terminal
              </Button>
              <Button
                size="sm"
                variant="ghost"
                colorScheme="yellow"
                leftIcon={<StopCircle size={14} />}
                onClick={onStop}
              >
                Stop
              </Button>
            </>
          )}
          {!isDeleted && (
            <Button
              size="sm"
              variant="ghost"
              colorScheme="red"
              leftIcon={<Trash2 size={14} />}
              onClick={onDelete}
            >
              Delete
            </Button>
          )}
        </HStack>
      </VStack>
    </Box>
  );
}
