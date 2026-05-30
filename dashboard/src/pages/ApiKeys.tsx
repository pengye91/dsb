// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React, { useState, useEffect } from 'react';
import {
  Box,
  VStack,
  HStack,
  Text,
  Button,
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
  Badge,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalFooter,
  ModalBody,
  ModalCloseButton,
  Input,
  Textarea,
  NumberInput,
  NumberInputField,
  useDisclosure,
  useToast,
  IconButton,
  Tooltip,
  Code,
  Alert,
  AlertIcon,
  AlertTitle,
  AlertDescription,
  Flex,
} from '@chakra-ui/react';
import {
  Plus,
  Eye,
  EyeOff,
  Copy,
  Trash2,
  RefreshCw,
  KeyRound,
} from 'lucide-react';
import { useAdminApiKey } from '../hooks/useAdminApiKey';

const BASE_PATH = import.meta.env.VITE_BASE_PATH || '';

interface ApiKey {
  id: string;
  key_hash: string;
  key_prefix: string;
  name: string;
  description?: string;
  scopes: string[];
  is_active: boolean;
  created_at: string;
  expires_at?: string;
  last_used_at?: string;
}

interface CreateApiKeyResponse {
  api_key: string;
  key: ApiKey;
}

export default function ApiKeys() {
  const { adminApiKey, isLoaded: isAdminKeyLoaded } = useAdminApiKey();
  const [apiKeys, setApiKeys] = useState<ApiKey[]>([]);
  const [loading, setLoading] = useState(true);
  const [revealedKeys, setRevealedKeys] = useState<Set<string>>(new Set());
  const [newKey, setNewKey] = useState<string | null>(null);
  const toast = useToast();

  // Debug logging on mount
  useEffect(() => {
    console.log('[ApiKeys] Component mounted');
    console.log('[ApiKeys] isAdminKeyLoaded:', isAdminKeyLoaded);
    console.log('[ApiKeys] adminApiKey:', adminApiKey ? '***' : '(empty)');
    console.log('[ApiKeys] localStorage admin key:', localStorage.getItem('dsb_admin_api_key') ? '***' : '(none)');
  }, [isAdminKeyLoaded, adminApiKey]);

  const {
    isOpen: isCreateModalOpen,
    onOpen: onCreateModalOpen,
    onClose: onCreateModalClose,
  } = useDisclosure();

  const {
    isOpen: isDeleteModalOpen,
    onOpen: onDeleteModalOpen,
    onClose: onDeleteModalClose,
  } = useDisclosure();

  const [keyToDelete, setKeyToDelete] = useState<ApiKey | null>(null);

  // Form state
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [scopes, setScopes] = useState('');
  const [expiresInDays, setExpiresInDays] = useState<number>(365);

  const fetchApiKeys = async () => {
    // Guard: Don't fetch if admin API key is not set
    if (!adminApiKey) {
      console.log('[ApiKeys] fetchApiKeys called but adminApiKey is empty, skipping fetch');
      setLoading(false);
      return;
    }

    console.log('[ApiKeys] Fetching API keys with admin key');
    try {
      const response = await fetch(`${BASE_PATH}/admin/api-keys`, {
        headers: {
          'X-API-Key': adminApiKey,
        },
      });

      if (response.ok) {
        const data = await response.json();
        setApiKeys(data);
      } else if (response.status === 401) {
        toast({
          title: 'Unauthorized',
          description: 'Admin API key required',
          status: 'error',
          duration: 5000,
        });
      } else {
        toast({
          title: 'Error',
          description: 'Failed to fetch API keys',
          status: 'error',
          duration: 5000,
        });
      }
    } catch (error) {
      console.error('Error fetching API keys:', error);
      toast({
        title: 'Error',
        description: 'Failed to connect to server',
        status: 'error',
        duration: 5000,
      });
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    // Only fetch if admin API key is loaded and configured
    if (isAdminKeyLoaded && adminApiKey) {
      console.log('[ApiKeys] Admin key loaded, fetching API keys');
      fetchApiKeys();
    } else if (isAdminKeyLoaded && !adminApiKey) {
      console.log('[ApiKeys] Admin key loaded but empty, showing error');
      setLoading(false);
    }
  }, [adminApiKey, isAdminKeyLoaded]);

  const handleCreate = async () => {
    if (!name.trim()) {
      toast({
        title: 'Validation Error',
        description: 'Name is required',
        status: 'error',
        duration: 3000,
      });
      return;
    }

    try {
      const body: any = {
        name: name.trim(),
      };

      if (description.trim()) {
        body.description = description.trim();
      }

      if (scopes.trim()) {
        body.scopes = scopes.split(',').map((s) => s.trim()).filter(Boolean);
      }

      if (expiresInDays > 0) {
        body.expires_in_days = expiresInDays;
      }

      const response = await fetch(`${BASE_PATH}/admin/api-keys`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-API-Key': adminApiKey || '',
        },
        body: JSON.stringify(body),
      });

      if (response.ok) {
        const data: CreateApiKeyResponse = await response.json();
        setNewKey(data.api_key);
        onCreateModalClose();
        fetchApiKeys();

        // Reset form
        setName('');
        setDescription('');
        setScopes('');
        setExpiresInDays(365);
      } else {
        const error = await response.json();
        toast({
          title: 'Error',
          description: error.message || 'Failed to create API key',
          status: 'error',
          duration: 5000,
        });
      }
    } catch (error) {
      console.error('Error creating API key:', error);
      toast({
        title: 'Error',
        description: 'Failed to create API key',
        status: 'error',
        duration: 5000,
      });
    }
  };

  const handleDelete = async () => {
    if (!keyToDelete) return;

    try {
      const response = await fetch(`${BASE_PATH}/admin/api-keys/${keyToDelete.id}`, {
        method: 'DELETE',
        headers: {
          'X-API-Key': adminApiKey || '',
        },
      });

      if (response.ok) {
        toast({
          title: 'Success',
          description: 'API key deleted successfully',
          status: 'success',
          duration: 3000,
        });
        onDeleteModalClose();
        fetchApiKeys();
      } else {
        toast({
          title: 'Error',
          description: 'Failed to delete API key',
          status: 'error',
          duration: 5000,
        });
      }
    } catch (error) {
      console.error('Error deleting API key:', error);
      toast({
        title: 'Error',
        description: 'Failed to delete API key',
        status: 'error',
        duration: 5000,
      });
    }
  };

  const handleRotate = async (key: ApiKey) => {
    try {
      const response = await fetch(`${BASE_PATH}/admin/api-keys/${key.id}/rotate`, {
        method: 'POST',
        headers: {
          'X-API-Key': adminApiKey || '',
        },
      });

      if (response.ok) {
        const data: CreateApiKeyResponse = await response.json();
        setNewKey(data.api_key);
        fetchApiKeys();

        toast({
          title: 'Success',
          description: 'API key rotated successfully',
          status: 'success',
          duration: 3000,
        });
      } else {
        toast({
          title: 'Error',
          description: 'Failed to rotate API key',
          status: 'error',
          duration: 5000,
        });
      }
    } catch (error) {
      console.error('Error rotating API key:', error);
      toast({
        title: 'Error',
        description: 'Failed to rotate API key',
        status: 'error',
        duration: 5000,
      });
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    toast({
      title: 'Copied',
      description: 'API key copied to clipboard',
      status: 'success',
      duration: 2000,
    });
  };

  const formatDate = (dateString: string) => {
    const date = new Date(dateString);
    return date.toLocaleString();
  };

  const isExpired = (expiresAt?: string) => {
    if (!expiresAt) return false;
    return new Date(expiresAt) < new Date();
  };

  // Show loading while admin API key is being loaded
  if (!isAdminKeyLoaded) {
    return (
      <Box p={8}>
        <Text>Loading...</Text>
      </Box>
    );
  }

  // Show error if admin API key is not configured
  if (!adminApiKey) {
    return (
      <Box p={8}>
        <VStack spacing={6} align="stretch">
          <Alert status="error" variant="subtle">
            <AlertIcon />
            <Box flex={1}>
              <AlertTitle>Admin API Key Required</AlertTitle>
              <AlertDescription>
                <VStack align="start" spacing={3}>
                  <Text>
                    API key management operations require the admin API key to be
                    configured in Settings.
                  </Text>
                  <Text fontSize="sm" color="gray.500">
                    Current value: {adminApiKey ? '***' : '(none)'}
                  </Text>
                  <HStack>
                    <Button
                      colorScheme="blue"
                      onClick={() => (window.location.href = `${BASE_PATH}/settings`)}
                    >
                      Go to Settings
                    </Button>
                  </HStack>
                </VStack>
              </AlertDescription>
            </Box>
          </Alert>
        </VStack>
      </Box>
    );
  }

  return (
    <Box p={8}>
      <VStack spacing={6} align="stretch">
        <Flex justifyContent="space-between" alignItems="center">
          <Box>
            <HStack spacing={3}>
              <KeyRound size={24} />
              <Text fontSize="2xl" fontWeight="bold">
                API Keys
              </Text>
            </HStack>
            <Text color="gray.500" mt={1}>
              Manage API keys for accessing the DSB API
            </Text>
          </Box>
          <Button
            leftIcon={<Plus size={18} />}
            colorScheme="blue"
            onClick={onCreateModalOpen}
          >
            Create API Key
          </Button>
        </Flex>

        {newKey && (
          <Alert status="success" variant="subtle">
            <AlertIcon />
            <Box flex={1}>
              <AlertTitle>API Key Created Successfully!</AlertTitle>
              <AlertDescription>
                <VStack align="start" spacing={3}>
                  <Text>
                    ⚠️ Save this key now - you won't be able to see it again!
                  </Text>
                  <HStack>
                    <Code p={2} bg="gray.100" borderRadius="md">
                      {newKey}
                    </Code>
                    <IconButton
                      icon={<Copy size={16} />}
                      aria-label="Copy API key"
                      size="sm"
                      onClick={() => copyToClipboard(newKey)}
                    />
                  </HStack>
                  <Button size="sm" onClick={() => setNewKey(null)}>
                    Dismiss
                  </Button>
                </VStack>
              </AlertDescription>
            </Box>
          </Alert>
        )}

        {loading ? (
          <Text>Loading...</Text>
        ) : apiKeys.length === 0 ? (
          <Box
            textAlign="center"
            py={12}
            bg="white"
            borderRadius="lg"
            borderWidth="1px"
          >
            <KeyRound size={48} className="mx-auto mb-4 text-gray-400" />
            <Text fontSize="xl" mb={2}>
              No API keys yet
            </Text>
            <Text color="gray.500" mb={4}>
              Create your first API key to get started
            </Text>
            <Button
              leftIcon={<Plus size={18} />}
              colorScheme="blue"
              onClick={onCreateModalOpen}
            >
              Create API Key
            </Button>
          </Box>
        ) : (
          <Table variant="simple" bg="white" borderRadius="lg">
            <Thead>
              <Tr>
                <Th>Name</Th>
                <Th>Key Prefix</Th>
                <Th>Status</Th>
                <Th>Created</Th>
                <Th>Last Used</Th>
                <Th>Expires</Th>
                <Th>Actions</Th>
              </Tr>
            </Thead>
            <Tbody>
              {apiKeys.map((key) => (
                <Tr key={key.id}>
                  <Td>
                    <VStack align="start" spacing={0}>
                      <Text fontWeight="medium">{key.name}</Text>
                      {key.description && (
                        <Text fontSize="sm" color="gray.500">
                          {key.description}
                        </Text>
                      )}
                    </VStack>
                  </Td>
                  <Td>
                    <Code fontSize="sm">{key.key_prefix}</Code>
                  </Td>
                  <Td>
                    <Badge
                      colorScheme={
                        !key.is_active
                          ? 'gray'
                          : isExpired(key.expires_at)
                          ? 'red'
                          : 'green'
                      }
                    >
                      {!key.is_active
                        ? 'Inactive'
                        : isExpired(key.expires_at)
                        ? 'Expired'
                        : 'Active'}
                    </Badge>
                  </Td>
                  <Td>{formatDate(key.created_at)}</Td>
                  <Td>
                    {key.last_used_at
                      ? formatDate(key.last_used_at)
                      : 'Never'}
                  </Td>
                  <Td>
                    {key.expires_at ? formatDate(key.expires_at) : 'Never'}
                  </Td>
                  <Td>
                    <HStack spacing={2}>
                      <Tooltip label="Rotate">
                        <IconButton
                          icon={<RefreshCw size={16} />}
                          aria-label="Rotate key"
                          size="sm"
                          variant="ghost"
                          onClick={() => handleRotate(key)}
                        />
                      </Tooltip>
                      <Tooltip label="Delete">
                        <IconButton
                          icon={<Trash2 size={16} />}
                          aria-label="Delete key"
                          size="sm"
                          variant="ghost"
                          colorScheme="red"
                          onClick={() => {
                            setKeyToDelete(key);
                            onDeleteModalOpen();
                          }}
                        />
                      </Tooltip>
                    </HStack>
                  </Td>
                </Tr>
              ))}
            </Tbody>
          </Table>
        )}
      </VStack>

      {/* Create Modal */}
      <Modal
        isOpen={isCreateModalOpen}
        onClose={onCreateModalClose}
        size="lg"
      >
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Create API Key</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <VStack spacing={4}>
              <Box w="100%">
                <Text mb={2} fontWeight="medium">
                  Name <Text as="span" color="red.500">*</Text>
                </Text>
                <Input
                  placeholder="My API Key"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </Box>

              <Box w="100%">
                <Text mb={2} fontWeight="medium">
                  Description
                </Text>
                <Textarea
                  placeholder="Optional description"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                />
              </Box>

              <Box w="100%">
                <Text mb={2} fontWeight="medium">
                  Scopes (comma-separated)
                </Text>
                <Input
                  placeholder="sandbox:read, sandbox:write"
                  value={scopes}
                  onChange={(e) => setScopes(e.target.value)}
                />
                <Text fontSize="sm" color="gray.500" mt={1}>
                  Example: sandbox:read, sandbox:write
                </Text>
              </Box>

              <Box w="100%">
                <Text mb={2} fontWeight="medium">
                  Expires In (days)
                </Text>
                <NumberInput
                  min={1}
                  max={3650}
                  value={expiresInDays}
                  onChange={(value) => setExpiresInDays(parseInt(value))}
                >
                  <NumberInputField />
                </NumberInput>
              </Box>
            </VStack>
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" onClick={onCreateModalClose}>
              Cancel
            </Button>
            <Button colorScheme="blue" onClick={handleCreate}>
              Create
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>

      {/* Delete Confirmation Modal */}
      <Modal
        isOpen={isDeleteModalOpen}
        onClose={onDeleteModalClose}
        size="md"
      >
        <ModalOverlay />
        <ModalContent>
          <ModalHeader>Delete API Key</ModalHeader>
          <ModalCloseButton />
          <ModalBody>
            <VStack spacing={4}>
              <Text>
                Are you sure you want to delete the API key{' '}
                <Text as="strong" color="red.500">
                  "{keyToDelete?.name}"
                </Text>
                ?
              </Text>
              <Text color="gray.500" fontSize="sm">
                This action cannot be undone. Any applications using this key
                will no longer be able to access the API.
              </Text>
            </VStack>
          </ModalBody>
          <ModalFooter>
            <Button variant="ghost" onClick={onDeleteModalClose}>
              Cancel
            </Button>
            <Button colorScheme="red" onClick={handleDelete}>
              Delete
            </Button>
          </ModalFooter>
        </ModalContent>
      </Modal>
    </Box>
  );
}
