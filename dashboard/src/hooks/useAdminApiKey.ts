// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { useState, useEffect } from 'react';
import { apiClient } from '../api/client';

const ADMIN_API_KEY_STORAGE = 'dsb_admin_api_key';

export function useAdminApiKey() {
  const [adminApiKey, setAdminApiKey] = useState<string>('');
  const [isLoaded, setIsLoaded] = useState(false);

  useEffect(() => {
    // Load admin API key from localStorage on mount
    const storedKey = localStorage.getItem(ADMIN_API_KEY_STORAGE);
    console.log('[useAdminApiKey] Loading from localStorage:', storedKey ? '***' : '(none)');
    if (storedKey) {
      setAdminApiKey(storedKey);
      apiClient.setAdminApiKey(storedKey);
      console.log('[useAdminApiKey] ✓ Admin API key loaded and set in apiClient');
    }
    setIsLoaded(true);
  }, []);

  const updateAdminApiKey = (newKey: string) => {
    console.log('[useAdminApiKey] Setting new admin API key:', newKey ? '***' : '(empty)');
    setAdminApiKey(newKey);
    if (newKey) {
      localStorage.setItem(ADMIN_API_KEY_STORAGE, newKey);
    } else {
      localStorage.removeItem(ADMIN_API_KEY_STORAGE);
    }
    apiClient.setAdminApiKey(newKey);
    console.log('[useAdminApiKey] ✓ Admin API key saved to localStorage and apiClient');
  };

  const clearAdminApiKey = () => {
    console.log('[useAdminApiKey] Clearing admin API key');
    setAdminApiKey('');
    localStorage.removeItem(ADMIN_API_KEY_STORAGE);
    apiClient.setAdminApiKey('');
  };

  return {
    adminApiKey,
    setAdminApiKey: updateAdminApiKey,
    clearAdminApiKey,
    isLoaded,
  };
}
