// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import { Outlet, useNavigate } from 'react-router-dom';
import { Box, Flex } from '@chakra-ui/react';
import { Header } from './Header';
import { Sidebar } from './Sidebar';

interface LayoutProps {
  apiKey?: string;
  adminApiKey?: string;
}

export function Layout({ apiKey, adminApiKey }: LayoutProps) {
  const navigate = useNavigate();

  const handleOpenSettings = () => {
    console.log('[Layout] Navigating to /settings');
    navigate('/settings');
  };

  return (
    <Box minH="100vh" display="flex" flexDirection="column">
      <Header onOpenSettings={handleOpenSettings} apiKey={apiKey || adminApiKey} />

      <Flex flex={1}>
        <Sidebar />
        <Box flex={1} p={6} bg="gray.50" overflow="auto">
          <Outlet />
        </Box>
      </Flex>
    </Box>
  );
}
