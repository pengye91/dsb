// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import React from 'react';
import { Box, VStack, Text, Flex, useColorModeValue } from '@chakra-ui/react';
import {
  LayoutDashboard,
  Container,
  Package,
  FileText,
  Terminal,
  Monitor,
  KeyRound,
} from 'lucide-react';
import { NavLink } from 'react-router-dom';

interface NavItem {
  path: string;
  label: string;
  icon: React.ReactNode;
}

const navItems: NavItem[] = [
  { path: '/', label: 'Dashboard', icon: <LayoutDashboard size={18} /> },
  { path: '/sandboxes', label: 'Sandboxes', icon: <Container size={18} /> },
  { path: '/sandboxes/create', label: 'Create Sandbox', icon: <Container size={18} /> },
  { path: '/images', label: 'Images', icon: <Package size={18} /> },
  { path: '/activities', label: 'Activities', icon: <FileText size={18} /> },
  { path: '/api-keys', label: 'API Keys', icon: <KeyRound size={18} /> },
];

interface SidebarProps {
  onClose?: () => void;
}

export function Sidebar({ onClose }: SidebarProps) {
  const bg = useColorModeValue('gray.900', 'gray.900');
  const hoverBg = useColorModeValue('gray.700', 'gray.700');
  const activeBg = useColorModeValue('blue.600', 'blue.600');

  return (
    <Box
      as="nav"
      w="250px"
      bg={bg}
      color="white"
      p={4}
      h="100vh"
      position="sticky"
      top={0}
    >
      <VStack align="stretch" spacing={2}>
        {navItems.map((item) => (
          <NavLink
            key={item.path}
            to={item.path}
            onClick={onClose}
            end={item.path === '/sandboxes'} // Only match exact path for /sandboxes
            style={({ isActive }) => ({
              textDecoration: 'none',
            })}
          >
            {({ isActive }) => (
              <Flex
                align="center"
                gap={3}
                p={3}
                borderRadius="md"
                bg={isActive ? activeBg : 'transparent'}
                color="white"
                _hover={{
                  bg: isActive ? activeBg : hoverBg,
                }}
                transition="all 0.2s"
                cursor="pointer"
              >
                {item.icon}
                <Text fontSize="sm" fontWeight={isActive ? 'bold' : 'normal'}>
                  {item.label}
                </Text>
              </Flex>
            )}
          </NavLink>
        ))}
      </VStack>
    </Box>
  );
}
