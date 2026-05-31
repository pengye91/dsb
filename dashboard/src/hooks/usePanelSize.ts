// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { useState, useEffect } from 'react';

const PANEL_SIZE_STORAGE = 'dsb_minifs_panel_size';
const DEFAULT_PANEL_SIZE = 40; // percentage
const MIN_PANEL_SIZE = 20; // percentage
const MAX_PANEL_SIZE = 80; // percentage

/**
 * Custom hook for managing MiniFs panel size with localStorage persistence
 * Enforces 20%-80% size constraints for file explorer panel
 */
export function usePanelSize() {
  const [panelSize, setPanelSizeState] = useState<number>(DEFAULT_PANEL_SIZE);
  const [isLoaded, setIsLoaded] = useState(false);

  // Load from localStorage on mount
  useEffect(() => {
    try {
      const storedSize = localStorage.getItem(PANEL_SIZE_STORAGE);
      if (storedSize) {
        const parsedSize = parseFloat(storedSize);
        // Validate stored size is within constraints
        if (parsedSize >= MIN_PANEL_SIZE && parsedSize <= MAX_PANEL_SIZE) {
          setPanelSizeState(parsedSize);
        } else {
          // Reset to default if out of bounds
          setPanelSizeState(DEFAULT_PANEL_SIZE);
        }
      }
    } catch (error) {
      console.warn('Failed to access localStorage:', error);
      setPanelSizeState(DEFAULT_PANEL_SIZE);
    } finally {
      setIsLoaded(true);
    }
  }, []);

  const setPanelSize = (newSize: number) => {
    // Enforce constraints
    const constrainedSize = Math.max(
      MIN_PANEL_SIZE,
      Math.min(MAX_PANEL_SIZE, newSize)
    );

    setPanelSizeState(constrainedSize);

    try {
      localStorage.setItem(PANEL_SIZE_STORAGE, constrainedSize.toString());
    } catch (error) {
      console.warn('Failed to persist panel size:', error);
    }
  };

  const resetPanelSize = () => {
    setPanelSize(DEFAULT_PANEL_SIZE);
  };

  return {
    leftPanelSize: panelSize,
    setLeftPanelSize: setPanelSize,
    resetPanelSize,
    isLoaded,
  };
}
