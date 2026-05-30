// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { useState, useEffect } from 'react';
import { apiClient } from '../api/client';

const USER_API_KEY_STORAGE = 'dsb_api_key';

export function useApiKey() {
  const [apiKey, setApiKey] = useState<string>('');
  const [isLoaded, setIsLoaded] = useState(false);

  useEffect(() => {
    // Load API key from localStorage on mount
    const storedKey = localStorage.getItem(USER_API_KEY_STORAGE);
    console.log('[useApiKey] Loading from localStorage:', storedKey ? '***' : '(none)');
    if (storedKey) {
      setApiKey(storedKey);
      apiClient.setApiKey(storedKey);
      console.log('[useApiKey] ✓ API key loaded and set in apiClient');
    }
    setIsLoaded(true);
  }, []);

  const updateApiKey = (newKey: string) => {
    console.log('[useApiKey] Setting new API key:', newKey ? '***' : '(empty)');
    setApiKey(newKey);
    if (newKey) {
      localStorage.setItem(USER_API_KEY_STORAGE, newKey);
    } else {
      localStorage.removeItem(USER_API_KEY_STORAGE);
    }
    apiClient.setApiKey(newKey);
    console.log('[useApiKey] ✓ API key saved to localStorage and apiClient');
  };

  const clearApiKey = () => {
    console.log('[useApiKey] Clearing API key');
    setApiKey('');
    localStorage.removeItem(USER_API_KEY_STORAGE);
    apiClient.setApiKey('');
  };

  return {
    apiKey,
    setApiKey: updateApiKey,
    clearApiKey,
    isLoaded,
  };
}
