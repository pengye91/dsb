// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
/**
 * VNC Utility Functions
 *
 * Provides utilities for parsing, validating, and formatting VNC resolution configurations.
 */

/**
 * VNC Resolution type with width and height
 */
export interface VNCResolution {
  width: number;
  height: number;
}

/**
 * Standard VNC resolution presets
 */
export const VNC_RESOLUTION_PRESETS: Record<string, VNCResolution> = {
  '720p': { width: 1280, height: 720 },
  '1080p': { width: 1920, height: 1080 },
  '1440p': { width: 2560, height: 1440 },
  '4K': { width: 3840, height: 2160 },
};

/**
 * Parse VNC resolution from string format "WIDTHxHEIGHT"
 *
 * @param resolution - Resolution string (e.g., "1920x1080", "2560x1440")
 * @returns Parsed resolution or null if invalid
 *
 * @example
 * ```ts
 * parseVNCResolution("1920x1080") // { width: 1920, height: 1080 }
 * parseVNCResolution("invalid") // null
 * ```
 */
export function parseVNCResolution(resolution: string): VNCResolution | null {
  if (!resolution) return null;

  const match = resolution.match(/^(\d+)x(\d+)$/);
  if (!match) return null;

  const width = parseInt(match[1], 10);
  const height = parseInt(match[2], 10);

  if (isNaN(width) || isNaN(height)) return null;

  // Validate reasonable resolution range
  if (width < 800 || width > 7680 || height < 600 || height < 4320) return null;

  return { width, height };
}

/**
 * Format VNC resolution to string "WIDTHxHEIGHT"
 *
 * @param resolution - Resolution object with width and height
 * @returns Formatted resolution string
 *
 * @example
 * ```ts
 * formatVNCResolution({ width: 1920, height: 1080 }) // "1920x1080"
 * ```
 */
export function formatVNCResolution(resolution: VNCResolution): string {
  return `${resolution.width}x${resolution.height}`;
}

/**
 * Validate VNC resolution string
 *
 * @param resolution - Resolution string to validate
 * @returns true if valid, false otherwise
 *
 * @example
 * ```ts
 * validateVNCResolution("1920x1080") // true
 * validateVNCResolution("invalid") // false
 * ```
 */
export function validateVNCResolution(resolution: string): boolean {
  return parseVNCResolution(resolution) !== null;
}

/**
 * Get default VNC resolution
 *
 * @returns Default resolution (2K/QHD: 2560x1440)
 */
export function getDefaultResolution(): VNCResolution {
  return { width: 2560, height: 1440 };
}

/**
 * Get human-readable label for resolution
 *
 * @param resolution - Resolution object
 * @returns Human-readable label (e.g., "Full HD (1920x1080)")
 */
export function getResolutionLabel(resolution: VNCResolution): string {
  const labels: Record<string, string> = {
    '1280x720': 'HD (1280x720)',
    '1920x1080': 'Full HD (1920x1080)',
    '2560x1440': '2K/QHD (2560x1440)',
    '3840x2160': '4K/UHD (3840x2160)',
  };

  const key = formatVNCResolution(resolution);
  return labels[key] || `${key}`;
}

/**
 * Get recommended resolution for display size
 *
 * @param displayWidth - Display width in pixels
 * @returns Recommended resolution
 */
export function getRecommendedResolution(displayWidth: number): VNCResolution {
  if (displayWidth >= 3840) return VNC_RESOLUTION_PRESETS['4K'];
  if (displayWidth >= 2560) return VNC_RESOLUTION_PRESETS['1440p'];
  if (displayWidth >= 1920) return VNC_RESOLUTION_PRESETS['1080p'];
  return VNC_RESOLUTION_PRESETS['720p'];
}
