// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React, { useEffect, useRef, useState } from 'react';
import {
  Heading,
  Box,
  VStack,
  HStack,
  Text,
  Button,
  SimpleGrid,
  useColorModeValue,
  useDisclosure,
  Spinner,
  Badge,
  Tabs,
  TabList,
  TabPanels,
  Tab,
  TabPanel,
  Alert,
  AlertIcon,
} from '@chakra-ui/react';
import {
  ArrowLeft,
  Terminal,
  Monitor,
  Activity,
  StopCircle,
  Trash2,
  RefreshCw,
  RotateCcw,
} from 'lucide-react';
import { Link, useParams, useNavigate } from 'react-router-dom';
import { apiClient } from '../api/client';
import { useSandboxStats } from '../hooks/useSandboxStats';
import { formatBytes, formatRelativeTime } from '../utils/formatters';
import type { Sandbox, ActivityResponse } from '../api/types';
import { VNCViewer, OpenVNCButton } from '../components/vnc';
import { SandboxActivities } from '../components/sandbox/SandboxActivities';
import { MiniFs } from '../components/sandbox/MiniFs';

export default function SandboxDetails() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [sandbox, setSandbox] = useState<Sandbox | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [terminalOpen, setTerminalOpen] = useState(false);
  const [activities, setActivities] = useState<ActivityResponse[]>([]);
  const [activitiesLoading, setActivitiesLoading] = useState(false);
  const [isRestoring, setIsRestoring] = useState(false);
  const containerStats = useSandboxStats(id || null, sandbox?.state === 'running');
  const terminalRef = useRef<HTMLDivElement>(null);
  const renderCount = useRef(0);
  const isInitialLoad = useRef(true);

  const bgColor = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');

  renderCount.current++;
  // Removed verbose render logging to reduce console noise

  const loadSandbox = async () => {
    if (!id) return;

    // Only show loading state on initial load, not on interval refreshes
    const isInitial = isInitialLoad.current;

    try {
      if (isInitial) {
        setLoading(true);
      }
      // Include deleted sandboxes so we can show details and restore button
      const data = await apiClient.getSandbox(id, true);
      setSandbox(data);
      setError(null);
    } catch (err: any) {
      setError(err.message || 'Failed to load sandbox');
    } finally {
      if (isInitial) {
        setLoading(false);
        isInitialLoad.current = false;
      }
    }
  };

  const loadActivities = async () => {
    if (!id) return;

    setActivitiesLoading(true);
    try {
      const data = await apiClient.getSandboxActivities(id, 50);
      setActivities(data);
    } catch (err: any) {
      console.error('Failed to load activities:', err);
    } finally {
      setActivitiesLoading(false);
    }
  };

  useEffect(() => {
    loadSandbox();
    loadActivities();
    // Refresh every 30 seconds (reduced from 5 seconds to reduce server load)
    const interval = setInterval(() => {
      // Silent refresh without loading state
      loadSandbox();
      loadActivities();
    }, 30000);
    return () => clearInterval(interval);
  }, [id]);

  const handleStop = async () => {
    if (!id || !confirm('Are you sure you want to stop this sandbox?')) return;

    try {
      await apiClient.stopSandbox(id);
      loadSandbox();
    } catch (err: any) {
      alert(`Failed to stop sandbox: ${err.message}`);
    }
  };

  const handleDelete = async () => {
    if (!id || !confirm('Are you sure you want to delete this sandbox?')) return;

    try {
      await apiClient.deleteSandbox(id);
      navigate('/sandboxes');
    } catch (err: any) {
      alert(`Failed to delete sandbox: ${err.message}`);
    }
  };

  const handleRestore = async () => {
    if (!id) return;

    setIsRestoring(true);
    try {
      await apiClient.restoreSandbox(id);
      await loadSandbox();
    } catch (err: any) {
      alert(`Failed to restore sandbox: ${err.message}`);
    } finally {
      setIsRestoring(false);
    }
  };

  if (loading) {
    return (
      <Box p={8} textAlign="center">
        <Spinner size="xl" />
        <Text mt={4}>Loading sandbox...</Text>
      </Box>
    );
  }

  if (error || !sandbox) {
    return (
      <Box p={4} bg="red.50" borderRadius="md">
        <Text color="red.500">{error || 'Sandbox not found'}</Text>
      </Box>
    );
  }

  const stateColors: Record<string, string> = {
    running: 'green',
    stopped: 'gray',
    error: 'red',
    creating: 'blue',
    starting: 'yellow',
    destroying: 'orange',
    destroyed: 'gray',
  };

  const isDeleted = sandbox.deleted_at !== null && sandbox.deleted_at !== undefined;

  return (
    <VStack align="stretch" spacing={6}>
      {/* Deleted Warning Banner */}
      {isDeleted && (
        <Alert status="warning" borderRadius="lg">
          <AlertIcon />
          <Box flex="1">
            <VStack align="start" spacing={1}>
              <Text fontWeight="bold">This sandbox has been deleted</Text>
              <Text fontSize="sm">
                Deleted {formatRelativeTime(sandbox.deleted_at)}
                {sandbox.deleted_by && ` by ${sandbox.deleted_by}`}
              </Text>
            </VStack>
          </Box>
          <Button
            leftIcon={<RotateCcw size={16} />}
            colorScheme="blue"
            size="sm"
            onClick={handleRestore}
            isLoading={isRestoring}
            isDisabled={isRestoring}
          >
            Restore Sandbox
          </Button>
        </Alert>
      )}

      {/* Header */}
      <HStack justify="space-between">
        <HStack>
          <Button as={Link} to="/sandboxes" variant="ghost" leftIcon={<ArrowLeft size={16} />}>
            Back
          </Button>
          <Heading size="lg">
            {sandbox.config.name || sandbox.id.slice(0, 8)}
          </Heading>
          <Badge colorScheme={stateColors[sandbox.state]} fontSize="md" px={3} py={1}>
            {sandbox.state}
          </Badge>
        </HStack>
        <HStack spacing={2}>
          <Button
            leftIcon={<RefreshCw size={16} />}
            variant="ghost"
            onClick={loadSandbox}
          >
            Refresh
          </Button>
          {isDeleted ? (
            <Button
              leftIcon={<RotateCcw size={16} />}
              variant="solid"
              colorScheme="blue"
              onClick={handleRestore}
              isLoading={isRestoring}
              isDisabled={isRestoring}
            >
              Restore
            </Button>
          ) : (
            <>
              {sandbox.state === 'running' && (
                <Button
                  leftIcon={<StopCircle size={16} />}
                  variant="ghost"
                  colorScheme="yellow"
                  onClick={handleStop}
                >
                  Stop
                </Button>
              )}
              <Button
                leftIcon={<Trash2 size={16} />}
                variant="ghost"
                colorScheme="red"
                onClick={handleDelete}
              >
                Delete
              </Button>
            </>
          )}
        </HStack>
      </HStack>

      {/* Info Cards */}
      <SimpleGrid columns={{ base: 1, md: 3 }} spacing={4}>
        <InfoCard label="Image" value={sandbox.config.image} />
        <InfoCard
          label="Created"
          value={formatRelativeTime(sandbox.created_at)}
        />
        <InfoCard
          label="Activity"
          value={`${sandbox.activity?.activity_count || 0} API calls`}
        />
      </SimpleGrid>

      {/* Real-time Stats (if running) */}
      {sandbox.state === 'running' && containerStats.stats && (
        <Box bg={bgColor} p={4} borderRadius="lg" borderWidth="1px" borderColor={borderColor}>
          <Heading size="md" mb={4}>
            <HStack>
              <Activity size={20} />
              <Text>Resource Usage</Text>
            </HStack>
          </Heading>
          <SimpleGrid columns={{ base: 2, md: 4 }} spacing={4}>
            <StatBox
              label="CPU"
              value={`${containerStats.stats.cpu_percent.toFixed(1)}%`}
            />
            <StatBox
              label="Memory"
              value={`${containerStats.stats.memory_usage_mb.toFixed(0)} MB`}
            />
            <StatBox
              label="Network RX"
              value={formatBytes(containerStats.stats.network_rx_bytes)}
            />
            <StatBox
              label="Network TX"
              value={formatBytes(containerStats.stats.network_tx_bytes)}
            />
          </SimpleGrid>
        </Box>
      )}

      {/* Kubernetes Info (if available) */}
      {sandbox.kubernetes && (
        <Box bg={bgColor} p={4} borderRadius="lg" borderWidth="1px" borderColor={borderColor}>
          <Heading size="md" mb={4}>
            <HStack>
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 2L2 7l10 5 10-5-10-5z" />
                <path d="M2 17l10 5 10-5" />
                <path d="M2 12l10 5 10-5" />
              </svg>
              <Text>Kubernetes</Text>
            </HStack>
          </Heading>
          <SimpleGrid columns={{ base: 1, md: 2 }} spacing={4}>
            {sandbox.kubernetes.node_name && (
              <InfoCard label="Node" value={sandbox.kubernetes.node_name} />
            )}
            {sandbox.kubernetes.pod_ip && (
              <InfoCard label="Pod IP" value={sandbox.kubernetes.pod_ip} />
            )}
            {sandbox.kubernetes.service_name && (
              <InfoCard label="Service" value={sandbox.kubernetes.service_name} />
            )}
            {sandbox.kubernetes.message && (
              <InfoCard label="Status" value={sandbox.kubernetes.message} />
            )}
          </SimpleGrid>
        </Box>
      )}

      {/* Main Content Tabs */}
      <Box bg={bgColor} borderRadius="lg" borderWidth="1px" borderColor={borderColor}>
        <Tabs>
          <TabList px={4} pt={4}>
            <Tab>Terminal</Tab>
            <Tab>VNC</Tab>
            <Tab>Mini-FS</Tab>
            <Tab>Activities</Tab>
            <Tab>Configuration</Tab>
          </TabList>

          <TabPanels p={4}>
            {/* Terminal Tab */}
            <TabPanel>
              {sandbox.state !== 'running' ? (
                <Alert status="warning">
                  <AlertIcon />
                  Sandbox must be running to access terminal
                </Alert>
              ) : (
                <TerminalComponent key={`terminal-${sandbox.id}`} sandboxId={sandbox.id} />
              )}
            </TabPanel>

            {/* VNC Tab */}
            <TabPanel>
              <VStack align="stretch" spacing={4}>
                {sandbox.state !== 'running' ? (
                  <Alert status="warning">
                    <AlertIcon />
                    Sandbox must be running to access VNC
                  </Alert>
                ) : (
                  <>
                    <HStack justify="space-between">
                      <Text fontWeight="bold" fontSize="lg">
                        VNC Viewer
                      </Text>
                      <HStack spacing={2}>
                        <OpenVNCButton sandbox={sandbox} />
                      </HStack>
                    </HStack>
                    <VNCViewer key={`vnc-${sandbox.id}`} sandbox={sandbox} />
                  </>
                )}
              </VStack>
            </TabPanel>

            {/* Mini-FS Tab */}
            <TabPanel>
              <MiniFs sandboxId={sandbox.id} />
            </TabPanel>

            {/* Activities Tab */}
            <TabPanel>
              <SandboxActivities activities={activities} loading={activitiesLoading} />
            </TabPanel>

            {/* Configuration Tab */}
            <TabPanel>
              <VStack align="stretch" spacing={4}>
                <ConfigSection label="Image" value={sandbox.config.image} />
                <ConfigSection
                  label="Pull Policy"
                  value={sandbox.config.pull_policy}
                />
                <ConfigSection
                  label="Features"
                  value={sandbox.config.features.join(', ') || 'None'}
                />
                <ConfigSection
                  label="Enable All Features"
                  value={sandbox.config.enable_all_features ? 'Yes' : 'No'}
                />
                <ConfigSection
                  label="Inactivity Timeout"
                  value={
                    sandbox.config.inactivity_timeout_minutes
                      ? `${sandbox.config.inactivity_timeout_minutes} minutes`
                      : 'Not set'
                  }
                />

                {sandbox.config.port_mappings.length > 0 && (
                  <Box>
                    <Text fontWeight="bold" mb={2}>
                      Port Mappings
                    </Text>
                    {sandbox.config.port_mappings.map((mapping, index) => (
                      <Text key={index} fontSize="sm">
                        {mapping.host_port} → {mapping.container_port}/{mapping.protocol}
                      </Text>
                    ))}
                  </Box>
                )}

                {sandbox.config.environment && Object.keys(sandbox.config.environment).length > 0 && (
                  <Box>
                    <Text fontWeight="bold" mb={2}>
                      Environment Variables
                    </Text>
                    {Object.entries(sandbox.config.environment).map(([key, value]) => (
                      <Text key={key} fontSize="sm">
                        {key}={value}
                      </Text>
                    ))}
                  </Box>
                )}

                {sandbox.config.volumes.length > 0 && (
                  <Box>
                    <Text fontWeight="bold" mb={2}>
                      Volumes
                    </Text>
                    {sandbox.config.volumes.map((volume, index) => (
                      <Text key={index} fontSize="sm">
                        {volume.host_path} → {volume.container_path}
                        {volume.read_only ? ' (read-only)' : ''}
                      </Text>
                    ))}
                  </Box>
                )}
              </VStack>
            </TabPanel>
          </TabPanels>
        </Tabs>
      </Box>
    </VStack>
  );
}

function InfoCard({ label, value }: { label: string; value: string }) {
  const bg = useColorModeValue('white', 'gray.800');
  const borderColor = useColorModeValue('gray.200', 'gray.700');

  return (
    <Box bg={bg} p={4} borderRadius="lg" borderWidth="1px" borderColor={borderColor}>
      <Text fontSize="sm" color="gray.500" mb={1}>
        {label}
      </Text>
      <Text fontWeight="semibold">{value}</Text>
    </Box>
  );
}

function StatBox({ label, value }: { label: string; value: string }) {
  return (
    <Box>
      <Text fontSize="xs" color="gray.500">
        {label}
      </Text>
      <Text fontWeight="bold" fontSize="lg">
        {value}
      </Text>
    </Box>
  );
}

function ConfigSection({ label, value }: { label: string; value: string | undefined }) {
  return (
    <HStack justify="space-between" borderBottom="1px" borderColor="gray.200" py={2}>
      <Text fontWeight="medium">{label}</Text>
      <Text color="gray.600">{value || 'N/A'}</Text>
    </HStack>
  );
}

const TerminalComponent = React.memo(function TerminalComponent({ sandboxId }: { sandboxId: string }) {
  const terminalRef = useRef<HTMLDivElement>(null);
  // Track the last initialized sandbox and timestamp to prevent StrictMode double-mount
  const lastInitRef = useRef<{ sandboxId: string; timestamp: number } | null>(null);

  useEffect(() => {
    // Skip if this same sandbox was initialized very recently (StrictMode double-mount prevention)
    const now = Date.now();
    if (lastInitRef.current?.sandboxId === sandboxId && (now - lastInitRef.current.timestamp) < 1000) {
      return;
    }

    let term: any;
    let fitAddon: any;
    let ws: WebSocket;
    let resizeHandler: (() => void) | null = null;
    let initTimeoutId: NodeJS.Timeout;
    let fitTimeoutId: NodeJS.Timeout;

    const initTerminal = async () => {
      if (!terminalRef.current) {
        return;
      }

      try {
        // Import xterm dynamically
        const { Terminal } = await import('@xterm/xterm');
        const { FitAddon } = await import('@xterm/addon-fit');

        // Import CSS
        await import('@xterm/xterm/css/xterm.css');

        // Create terminal
        term = new Terminal({
          cursorBlink: true,
          fontSize: 14,
          fontFamily: 'Monaco, "Courier New", monospace',
          theme: {
            background: '#1a1a1a',
            foreground: '#ffffff',
            cursor: '#ffffff',
          },
        });

        fitAddon = new FitAddon();
        term.loadAddon(fitAddon);

        // Open terminal in DOM
        term.open(terminalRef.current);

        // Schedule initial fit after terminal is fully rendered
        fitTimeoutId = setTimeout(() => {
          if (fitAddon && term.element) {
            try {
              fitAddon.fit();
            } catch (e) {
              // Silently ignore - will retry on resize
              console.debug('[Terminal] Initial fit failed (will retry on resize)');
            }
          }
        }, 500);

        // Connect to WebSocket with API key as query parameter
        // Use admin API key if available, otherwise use user API key
        const apiKey = localStorage.getItem('dsb_admin_api_key') || localStorage.getItem('dsb_api_key') || '';

        // Use current host and protocol for WebSocket connection
        const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsHost = window.location.host;
        const basePath = import.meta.env.VITE_BASE_PATH || '';
        const baseUrl = `${wsProtocol}//${wsHost}${basePath}/terminal/${sandboxId}`;

        const wsUrl = apiKey
          ? `${baseUrl}?api_key=${encodeURIComponent(apiKey)}`
          : baseUrl;

        ws = new WebSocket(wsUrl);

        ws.onopen = () => {
          if (term) {
            term.write('\r\n✓ Connected to sandbox terminal\r\n\n');
          }
        };

        ws.onmessage = (event) => {
          try {
            const message = JSON.parse(event.data);
            if (message.type === 'output' && term) {
              term.write(message.data);
            } else if (message.type === 'error' && term) {
              term.write(`\r\n✗ Error: ${message.data}\r\n`);
            } else if (message.type === 'end' && term) {
              term.write('\r\n✗ Session ended\r\n');
            }
          } catch (e) {
            // If not JSON, write directly
            if (term) {
              term.write(event.data);
            }
          }
        };

        ws.onerror = (error) => {
          console.error('[Terminal] WebSocket error:', error);
          if (term) {
            term.write('\r\n✗ Connection error (check console for details)\r\n');
          }
        };

        ws.onclose = (event) => {
          if (term) {
            term.write(`\r\n✗ Connection closed (code: ${event.code})\r\n`);
          }
        };

        term.onData((data: string) => {
          if (ws && ws.readyState === WebSocket.OPEN) {
            ws.send(
              JSON.stringify({
                type: 'input',
                data,
              })
            );
          }
        });

        // Handle resize
        resizeHandler = () => {
          if (fitAddon && term) {
            try {
              fitAddon.fit();
              if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(
                  JSON.stringify({
                    type: 'resize',
                    data: {
                      rows: term.rows,
                      cols: term.cols,
                    },
                  })
                );
              }
            } catch (e) {
              console.error('Error resizing terminal:', e);
            }
          }
        };

        window.addEventListener('resize', resizeHandler);

        // Mark as initialized
        lastInitRef.current = { sandboxId, timestamp: Date.now() };
      } catch (error) {
        console.error('Failed to initialize terminal:', error);
      }
    };

    // Small delay to ensure DOM is ready
    initTimeoutId = setTimeout(initTerminal, 100);

    return () => {
      // Clear timeouts
      clearTimeout(initTimeoutId);
      clearTimeout(fitTimeoutId);

      // Remove resize handler
      if (resizeHandler) {
        window.removeEventListener('resize', resizeHandler);
      }

      // Close WebSocket if it's open or connecting
      if (ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING)) {
        ws.close();
      }

      // Dispose terminal
      if (term) {
        try {
          term.dispose();
        } catch (e) {
          // Ignore disposal errors
        }
      }
    };
  }, [sandboxId]);

  return (
    <Box
      ref={terminalRef}
      bg="#1a1a1a"
      borderRadius="md"
      overflow="hidden"
      h="500px"
      w="100%"
    />
  );
});
