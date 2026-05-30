// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React, { useState, useEffect } from 'react';
import {
  Heading,
  Box,
  VStack,
  FormControl,
  FormLabel,
  Input,
  Button,
  Text,
  useColorModeValue,
  Alert,
  AlertIcon,
  HStack,
  NumberInput,
  NumberInputField,
  Textarea,
  Checkbox,
  SimpleGrid,
} from '@chakra-ui/react';
import { ArrowLeft, Plus, Trash2 } from 'lucide-react';
import { Link, useNavigate } from 'react-router-dom';
import { useSandboxes } from '../hooks/useSandboxes';
import { useConfig } from '../hooks/useConfig';
import { SandboxCreationProgress } from '../components/sandboxes/SandboxCreationProgress';
import type { SandboxConfig, PortMapping, VolumeMount, SandboxProgressEvent } from '../api/types';

export default function CreateSandbox() {
  const { createSandboxStream } = useSandboxes();
  const { config: serverConfig, loading: configLoading } = useConfig();
  const navigate = useNavigate();
  const bg = useColorModeValue('white', 'gray.800');

  const [config, setConfig] = useState<Partial<SandboxConfig>>({
    image: serverConfig?.default_sandbox_image || 'dsb/sandbox:latest',
    name: '',
    port_mappings: [],
    environment: {},
    volumes: [],
    pull_policy: 'missing',
    features: [],
    enable_all_features: false,
    inactivity_timeout_minutes: serverConfig?.default_inactivity_timeout || 30,
  });

  // Update config when server config loads
  useEffect(() => {
    if (serverConfig) {
      setConfig(prev => ({
        ...prev,
        image: serverConfig.default_sandbox_image,
        inactivity_timeout_minutes: serverConfig.default_inactivity_timeout,
      }));
    }
  }, [serverConfig]);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showProgress, setShowProgress] = useState(false);
  const [progressStatus, setProgressStatus] = useState('');
  const [progress, setProgress] = useState(0);
  const [isComplete, setIsComplete] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setShowProgress(true);
    setProgressStatus('Initializing...');
    setProgress(0);
    setIsComplete(false);

    const fullConfig: SandboxConfig = {
      image: config.image || serverConfig?.default_sandbox_image || 'dsb/sandbox:latest',
      name: config.name,
      port_mappings: config.port_mappings || [],
      environment: config.environment || {},
      resource_limits: config.resource_limits,
      volumes: config.volumes || [],
      inactivity_timeout_minutes: config.inactivity_timeout_minutes || serverConfig?.default_inactivity_timeout || 30,
      pull_policy: config.pull_policy || 'missing',
      features: config.features || [],
      enable_all_features: config.enable_all_features || false,
    };

    createSandboxStream(
      fullConfig,
      (event: SandboxProgressEvent) => {
        if (event.type === 'pulling') {
          setProgressStatus(`Pulling image: ${event.status || '...'}`);
          if (event.total && event.current) {
            setProgress(Math.round((event.current / event.total) * 100));
          } else {
            setProgress(0);
          }
        } else if (event.type === 'creating') {
          setProgressStatus('Creating container...');
          setProgress(100);
        } else if (event.type === 'starting') {
          setProgressStatus('Starting container services...');
          setProgress(100);
        }
      },
      (sandboxId: string) => {
        setProgressStatus('Sandbox ready!');
        setIsComplete(true);
        setTimeout(() => {
          navigate(`/sandboxes/${sandboxId}`);
        }, 500);
      },
      (err: Error) => {
        setError(err.message || 'Failed to create sandbox');
        setLoading(false);
        setIsComplete(true);
      }
    );
  };

  const addPortMapping = () => {
    setConfig({
      ...config,
      port_mappings: [
        ...(config.port_mappings || []),
        { host_port: 0, container_port: 8080, protocol: 'tcp' },
      ],
    });
  };

  const removePortMapping = (index: number) => {
    setConfig({
      ...config,
      port_mappings: config.port_mappings?.filter((_, i) => i !== index),
    });
  };

  const updatePortMapping = (index: number, field: keyof PortMapping, value: any) => {
    const updated = [...(config.port_mappings || [])];
    updated[index] = { ...updated[index], [field]: value };
    setConfig({ ...config, port_mappings: updated });
  };

  const addVolume = () => {
    setConfig({
      ...config,
      volumes: [
        ...(config.volumes || []),
        { host_path: '', container_path: '/data', read_only: false },
      ],
    });
  };

  const removeVolume = (index: number) => {
    setConfig({
      ...config,
      volumes: config.volumes?.filter((_, i) => i !== index),
    });
  };

  const updateVolume = (index: number, field: keyof VolumeMount, value: any) => {
    const updated = [...(config.volumes || [])];
    updated[index] = { ...updated[index], [field]: value };
    setConfig({ ...config, volumes: updated });
  };

  return (
    <VStack align="stretch" spacing={6}>
      {/* Header */}
      <HStack>
        <Button as={Link} to="/sandboxes" variant="ghost" leftIcon={<ArrowLeft size={16} />}>
          Back to Sandboxes
        </Button>
        <Heading size="lg">Create Sandbox</Heading>
      </HStack>

      <Box bg={bg} p={6} borderRadius="lg" borderWidth="1px">
        <form onSubmit={handleSubmit}>
          <VStack align="stretch" spacing={4}>
            {error && (
              <Alert status="error" borderRadius="md">
                <AlertIcon />
                {error}
              </Alert>
            )}

            <FormControl isRequired>
              <FormLabel>Image</FormLabel>
              <Input
                placeholder="e.g., dsb/sandbox:latest"
                value={config.image}
                onChange={(e) => setConfig({ ...config, image: e.target.value })}
              />
            </FormControl>

            <FormControl>
              <FormLabel>Name (optional)</FormLabel>
              <Input
                placeholder="e.g., my-sandbox"
                value={config.name}
                onChange={(e) => setConfig({ ...config, name: e.target.value })}
              />
            </FormControl>

            <SimpleGrid columns={2} spacing={4}>
              <FormControl>
                <FormLabel>Pull Policy</FormLabel>
                <Input
                  as="select"
                  value={config.pull_policy}
                  onChange={(e) =>
                    setConfig({ ...config, pull_policy: e.target.value as any })
                  }
                >
                  <option value="missing">Missing (pull if not local)</option>
                  <option value="always">Always (always pull)</option>
                  <option value="never">Never (use local only)</option>
                </Input>
              </FormControl>

              <FormControl>
                <FormLabel>Inactivity Timeout (minutes)</FormLabel>
                <NumberInput
                  value={config.inactivity_timeout_minutes}
                  onChange={(value) =>
                    setConfig({ ...config, inactivity_timeout_minutes: parseInt(value) })
                  }
                >
                  <NumberInputField />
                </NumberInput>
              </FormControl>
            </SimpleGrid>

            <FormControl>
              <FormLabel>Environment Variables (JSON)</FormLabel>
              <Textarea
                placeholder='{"KEY": "value"}'
                value={JSON.stringify(config.environment, null, 2)}
                onChange={(e) => {
                  try {
                    setConfig({ ...config, environment: JSON.parse(e.target.value) });
                  } catch {
                    // Invalid JSON, ignore
                  }
                }}
                fontFamily="monospace"
              />
            </FormControl>

            <FormControl>
              <Checkbox
                isChecked={config.enable_all_features}
                onChange={(e) =>
                  setConfig({ ...config, enable_all_features: e.target.checked })
                }
              >
                Enable all DSB features
              </Checkbox>
            </FormControl>

            {/* Port Mappings */}
            <Box>
              <HStack justify="space-between" mb={2}>
                <FormLabel mb={0}>Port Mappings</FormLabel>
                <Button size="sm" leftIcon={<Plus size={14} />} onClick={addPortMapping}>
                  Add Port
                </Button>
              </HStack>
              {config.port_mappings?.map((mapping, index) => (
                <HStack key={index} spacing={2} mb={2}>
                  <NumberInput
                    value={mapping.host_port}
                    onChange={(value) => updatePortMapping(index, 'host_port', parseInt(value))}
                    min={0}
                  >
                    <NumberInputField placeholder="Host Port" />
                  </NumberInput>
                  <Text>→</Text>
                  <NumberInput
                    value={mapping.container_port}
                    onChange={(value) =>
                      updatePortMapping(index, 'container_port', parseInt(value))
                    }
                    min={0}
                  >
                    <NumberInputField placeholder="Container Port" />
                  </NumberInput>
                  <Input
                    as="select"
                    value={mapping.protocol}
                    onChange={(e) => updatePortMapping(index, 'protocol', e.target.value)}
                    w="100px"
                  >
                    <option value="tcp">TCP</option>
                    <option value="udp">UDP</option>
                  </Input>
                  <Button
                    size="sm"
                    colorScheme="red"
                    variant="ghost"
                    onClick={() => removePortMapping(index)}
                  >
                    <Trash2 size={14} />
                  </Button>
                </HStack>
              ))}
            </Box>

            {/* Volumes */}
            <Box>
              <HStack justify="space-between" mb={2}>
                <FormLabel mb={0}>Volumes</FormLabel>
                <Button size="sm" leftIcon={<Plus size={14} />} onClick={addVolume}>
                  Add Volume
                </Button>
              </HStack>
              {config.volumes?.map((volume, index) => (
                <HStack key={index} spacing={2} mb={2}>
                  <Input
                    placeholder="Host Path"
                    value={volume.host_path}
                    onChange={(e) => updateVolume(index, 'host_path', e.target.value)}
                  />
                  <Text>→</Text>
                  <Input
                    placeholder="Container Path"
                    value={volume.container_path}
                    onChange={(e) => updateVolume(index, 'container_path', e.target.value)}
                  />
                  <Checkbox
                    isChecked={volume.read_only}
                    onChange={(e) => updateVolume(index, 'read_only', e.target.checked)}
                  >
                    RO
                  </Checkbox>
                  <Button
                    size="sm"
                    colorScheme="red"
                    variant="ghost"
                    onClick={() => removeVolume(index)}
                  >
                    <Trash2 size={14} />
                  </Button>
                </HStack>
              ))}
            </Box>

            <HStack justify="flex-end" pt={4}>
              <Button
                as={Link}
                to="/sandboxes"
                variant="ghost"
                onClick={() => navigate('/sandboxes')}
              >
                Cancel
              </Button>
              <Button type="submit" colorScheme="blue" isLoading={loading}>
                Create Sandbox
              </Button>
            </HStack>
          </VStack>
        </form>
      </Box>

      <SandboxCreationProgress
        isOpen={showProgress}
        onClose={() => {
          setShowProgress(false);
          if (error) setLoading(false);
        }}
        status={progressStatus}
        progress={progress}
        error={error}
        isComplete={isComplete}
      />
    </VStack>
  );
}
