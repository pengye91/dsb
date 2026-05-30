// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
/**
 * Standalone VNC Viewer Page
 *
 * Full-page VNC viewer for standalone use in a new browser tab.
 * Provides a dedicated, full-screen VNC experience.
 */

import React, { useEffect, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Box, VStack, Heading, HStack, Text, Button, Spinner, Alert, AlertIcon } from '@chakra-ui/react';
import { ArrowLeft, X } from 'lucide-react';
import { apiClient } from '../api/client';
import { VNCViewer, VNCControls } from '../components/vnc';
import type { Sandbox } from '../api/types';

export default function StandaloneVNC() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [sandbox, setSandbox] = useState<Sandbox | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Fetch sandbox details
  useEffect(() => {
    if (!id) return;

    const loadSandbox = async () => {
      try {
        setLoading(true);
        const data = await apiClient.getSandbox(id, false);
        setSandbox(data);
        setError(null);
      } catch (err: any) {
        setError(err.message || 'Failed to load sandbox');
      } finally {
        setLoading(false);
      }
    };

    loadSandbox();
  }, [id]);

  // Handle close button (may not work in all browsers due to security)
  const handleClose = () => {
    if (window.opener) {
      window.close();
    } else {
      // If not opened by another window, navigate back to sandbox details
      navigate(`/sandboxes/${id}`);
    }
  };

  // Handle reconnection
  const handleReconnect = () => {
    // Force remount of VNCViewer by updating state
    setSandbox(null);
    setTimeout(() => {
      // Reload sandbox data
      if (id) {
        apiClient.getSandbox(id, false)
          .then((data) => setSandbox(data))
          .catch((err) => setError(err.message));
      }
    }, 100);
  };

  // Handle fullscreen
  const handleFullscreen = () => {
    if (document.documentElement.requestFullscreen) {
      document.documentElement.requestFullscreen();
    }
  };

  // Loading state
  if (loading) {
    return (
      <Box h="100vh" display="flex" alignItems="center" justifyContent="center" bg="gray.50">
        <VStack spacing={4}>
          <Spinner size="xl" thickness="4" color="blue.500" />
          <Text fontSize="lg" fontWeight="medium">
            Loading VNC Viewer...
          </Text>
        </VStack>
      </Box>
    );
  }

  // Error state
  if (error || !sandbox) {
    return (
      <Box h="100vh" display="flex" alignItems="center" justifyContent="center" bg="gray.50" p={8}>
        <VStack maxW="500px" spacing={4}>
          <Alert status="error">
            <AlertIcon />
            <VStack align="start" spacing={2}>
              <Text fontWeight="bold">Failed to Load Sandbox</Text>
              <Text fontSize="sm">{error || 'Sandbox not found'}</Text>
            </VStack>
          </Alert>
          <Button
            leftIcon={<ArrowLeft size={16} />}
            onClick={() => navigate('/sandboxes')}
            colorScheme="blue"
          >
            Back to Sandboxes
          </Button>
        </VStack>
      </Box>
    );
  }

  // Sandbox not running
  if (sandbox.state !== 'running') {
    return (
      <Box h="100vh" display="flex" alignItems="center" justifyContent="center" bg="gray.50" p={8}>
        <VStack maxW="500px" spacing={4}>
          <Alert status="warning">
            <AlertIcon />
            <VStack align="start" spacing={2}>
              <Text fontWeight="bold">Sandbox Not Running</Text>
              <Text fontSize="sm">
                The sandbox must be running to access VNC. Current state:{' '}
                <strong>{sandbox.state}</strong>
              </Text>
            </VStack>
          </Alert>
          <HStack spacing={3}>
            <Button
              leftIcon={<ArrowLeft size={16} />}
              onClick={() => navigate(`/sandboxes/${sandbox.id}`)}
              variant="ghost"
            >
              Back to Sandbox
            </Button>
            <Button
              onClick={() => navigate(`/sandboxes/${sandbox.id}`)}
              colorScheme="blue"
            >
              View Sandbox Details
            </Button>
          </HStack>
        </VStack>
      </Box>
    );
  }

  return (
    <Box h="100vh" display="flex" flexDirection="column" bg="gray.900">
      {/* Header */}
      <HStack
        px={6}
        py={4}
        bg="gray.800"
        borderBottomWidth="1px"
        borderColor="gray.700"
        justify="space-between"
      >
        <HStack spacing={4}>
          <Button
            leftIcon={<ArrowLeft size={16} />}
            onClick={() => navigate(`/sandboxes/${sandbox.id}`)}
            variant="ghost"
            size="sm"
            colorScheme="white"
          >
            Back
          </Button>
          <Heading size="md" color="white">
            {sandbox.config.name || sandbox.id.slice(0, 8)} - VNC Viewer
          </Heading>
        </HStack>
        <HStack spacing={2}>
          <Button
            leftIcon={<X size={16} />}
            onClick={handleClose}
            variant="ghost"
            size="sm"
            colorScheme="white"
          >
            Close
          </Button>
        </HStack>
      </HStack>

      {/* VNC Viewer - Full Height */}
      <Box flex="1" display="flex" flexDirection="column" p={4} minH={0}>
        <VNCViewer
          sandbox={sandbox}
          fullHeight={true}
          showConnectionStatus={true}
          resolution={sandbox.config.vnc_resolution}
        />
      </Box>
    </Box>
  );
}
