// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import {
  Heading,
  Box,
  VStack,
  HStack,
  FormControl,
  FormLabel,
  Input,
  InputGroup,
  InputLeftElement,
  Button,
  Text,
  useColorModeValue,
  Alert,
  AlertIcon,
} from '@chakra-ui/react';
import { Key, Shield, ShieldAlert } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useApiKey } from '../hooks/useApiKey';
import { useAdminApiKey } from '../hooks/useAdminApiKey';

export default function Settings() {
  const navigate = useNavigate();
  const { apiKey, setApiKey, clearApiKey, isLoaded } = useApiKey();
  const { adminApiKey, setAdminApiKey, clearAdminApiKey } = useAdminApiKey();
  const [inputKey, setInputKey] = React.useState('');
  const [inputAdminKey, setInputAdminKey] = React.useState('');
  const [saved, setSaved] = React.useState(false);

  const bgColor = useColorModeValue('white', 'gray.800');

  // Sync input fields with loaded API keys
  React.useEffect(() => {
    if (isLoaded) {
      setInputKey(apiKey);
      setInputAdminKey(adminApiKey);
    }
  }, [isLoaded, apiKey, adminApiKey]);

  const handleSave = () => {
    console.log('[Settings] Saving API keys:', { inputKey: inputKey ? '***' : '(empty)', inputAdminKey: inputAdminKey ? '***' : '(empty)' });
    setApiKey(inputKey);
    setAdminApiKey(inputAdminKey);
    setSaved(true);

    // Wait for state updates and localStorage to persist, then refresh page
    // This ensures all components reload with the new API keys
    console.log('[Settings] Waiting for state updates...');
    setTimeout(() => {
      console.log('[Settings] Reloading page to apply new keys');
      window.location.href = import.meta.env.VITE_BASE_PATH || '/';
    }, 200);
  };

  const handleClear = () => {
    clearApiKey();
    clearAdminApiKey();
    setInputKey('');
    setInputAdminKey('');
    setSaved(true);
    setTimeout(() => setSaved(false), 3000);
  };

  if (!isLoaded) {
    return <Box>Loading...</Box>;
  }

  return (
    <VStack align="stretch" spacing={6} maxW="2xl">
      <Heading size="lg">Settings</Heading>

      {/* API Key Section */}
      <Box bg={bgColor} p={6} borderRadius="lg" borderWidth="1px">
        <VStack align="stretch" spacing={4}>
          <HStack>
            <Shield size={20} color="#3182ce" />
            <Heading size="md">API Authentication</Heading>
          </HStack>

          <Text fontSize="sm" color="gray.600">
            Configure your DSB API keys for authentication with the backend
            server. API keys are stored locally in your browser.
          </Text>

          {saved && (
            <Alert status="success" borderRadius="md">
              <AlertIcon />
              Settings saved successfully!
            </Alert>
          )}

          {/* User API Key */}
          <FormControl>
            <FormLabel>User API Key</FormLabel>
            <InputGroup>
              <InputLeftElement pointerEvents="none">
                <Key size={16} color="gray" />
              </InputLeftElement>
              <Input
                type="password"
                placeholder="Enter your DSB user API key"
                value={inputKey}
                onChange={(e) => setInputKey(e.target.value)}
                pl={10}
              />
            </InputGroup>
            <Text fontSize="xs" color="gray.500" mt={1}>
              Regular API key for sandbox operations and general access
            </Text>
          </FormControl>

          {/* Admin API Key */}
          <FormControl>
            <FormLabel>
              Admin API Key <Text as="span" color="red.500">*</Text>
            </FormLabel>
            <InputGroup>
              <InputLeftElement pointerEvents="none">
                <ShieldAlert size={16} color="gray" />
              </InputLeftElement>
              <Input
                type="password"
                placeholder="Enter your DSB admin API key"
                value={inputAdminKey}
                onChange={(e) => setInputAdminKey(e.target.value)}
                pl={10}
              />
            </InputGroup>
            <Text fontSize="xs" color="gray.500" mt={1}>
              Required for API key management operations (create, list, delete,
              rotate keys)
            </Text>
          </FormControl>

          <HStack justify="flex-end">
            {(apiKey || adminApiKey) && (
              <Button variant="ghost" colorScheme="red" onClick={handleClear}>
                Clear Keys
              </Button>
            )}
            <Button colorScheme="blue" onClick={handleSave}>
              Save Settings
            </Button>
          </HStack>
        </VStack>
      </Box>

      {/* Info Section */}
      <Box bg="blue.50" p={6} borderRadius="lg">
        <VStack align="stretch" spacing={2}>
          <Text fontWeight="bold" color="blue.700">
            About API Keys
          </Text>
          <Text fontSize="sm" color="blue.600">
            The DSB server supports two types of API keys:
          </Text>
          <VStack align="start" spacing={1} pl={4}>
            <Text fontSize="sm" color="blue.600">
              • <strong>User API Key:</strong> For sandbox operations and general
              API access
            </Text>
            <Text fontSize="sm" color="blue.600">
              • <strong>Admin API Key:</strong> For managing API keys (create,
              list, delete, rotate)
            </Text>
          </VStack>
          <Text fontSize="sm" color="blue.600" mt={2}>
            Status:
          </Text>
          <HStack spacing={4}>
            <Text fontSize="sm" color="blue.600">
              User API Key:{' '}
              {apiKey ? (
                <Text as="span" color="green.600" fontWeight="bold">
                  ✓ Configured
                </Text>
              ) : (
                <Text as="span" color="orange.600" fontWeight="bold">
                  Optional
                </Text>
              )}
            </Text>
            <Text fontSize="sm" color="blue.600">
              Admin API Key:{' '}
              {adminApiKey ? (
                <Text as="span" color="green.600" fontWeight="bold">
                  ✓ Configured
                </Text>
              ) : (
                <Text as="span" color="red.600" fontWeight="bold">
                  ✗ Not configured
                </Text>
              )}
            </Text>
          </HStack>
        </VStack>
      </Box>
    </VStack>
  );
}
