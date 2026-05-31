// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import { ChakraProvider, Box } from '@chakra-ui/react';
import { BrowserRouter as Router, Routes, Route, Navigate } from 'react-router-dom';
import { Layout } from './components/layout/Layout';
import { useApiKey } from './hooks/useApiKey';
import { useAdminApiKey } from './hooks/useAdminApiKey';
import Dashboard from './pages/Dashboard';
import Sandboxes from './pages/Sandboxes';
import CreateSandbox from './pages/CreateSandbox';
import SandboxDetails from './pages/SandboxDetails';
import StandaloneVNC from './pages/StandaloneVNC';
import Images from './pages/Images';
import Activities from './pages/Activities';
import ApiKeys from './pages/ApiKeys';
import Settings from './pages/Settings';

const BASE_PATH = import.meta.env.VITE_BASE_PATH || '';

function App() {
  const { apiKey, isLoaded } = useApiKey();
  const { adminApiKey } = useAdminApiKey();

  if (!isLoaded) {
    return (
      <ChakraProvider>
        <Box p={8}>Loading...</Box>
      </ChakraProvider>
    );
  }

  // If no API key AND no admin API key, show settings page only
  if (!apiKey && !adminApiKey) {
    return (
      <ChakraProvider>
        <Router basename={BASE_PATH}>
          <Box minH="100vh" bg="gray.50" p={6}>
            <Routes>
              <Route path="/settings" element={<Settings />} />
              <Route path="*" element={<Navigate to="/settings" replace />} />
            </Routes>
          </Box>
        </Router>
      </ChakraProvider>
    );
  }

  return (
    <ChakraProvider>
      <Router basename={BASE_PATH}>
        <Routes>
          <Route path="/" element={<Layout apiKey={apiKey} adminApiKey={adminApiKey} />}>
            <Route index element={<Dashboard />} />
            <Route path="sandboxes" element={<Sandboxes />} />
            <Route path="sandboxes/create" element={<CreateSandbox />} />
            <Route path="sandboxes/:id" element={<SandboxDetails />} />
            <Route path="images" element={<Images />} />
            <Route path="activities" element={<Activities />} />
            <Route path="api-keys" element={<ApiKeys />} />
            <Route path="settings" element={<Settings />} />
          </Route>
          {/* Standalone VNC viewer - outside Layout for full page */}
          {/* Uses /vnc-viewer/ to avoid conflict with nginx /vnc/ WebSocket proxy */}
          <Route path="/vnc-viewer/:id" element={<StandaloneVNC />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </Router>
    </ChakraProvider>
  );
}

export default App;
