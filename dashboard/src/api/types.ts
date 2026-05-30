// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
import { z } from 'zod';

// Sandbox State
export const SandboxStateSchema = z.enum([
  'creating',
  'created',
  'starting',
  'running',
  'stopped',
  'error',
  'destroying',
  'destroyed',
]);

export type SandboxState = z.infer<typeof SandboxStateSchema>;

// Port Mapping
export const PortMappingSchema = z.object({
  host_port: z.number(),
  container_port: z.number(),
  protocol: z.enum(['tcp', 'udp']),
});

export type PortMapping = z.infer<typeof PortMappingSchema>;

// Volume Mount
export const VolumeMountSchema = z.object({
  host_path: z.string(),
  container_path: z.string(),
  read_only: z.boolean(),
});

export type VolumeMount = z.infer<typeof VolumeMountSchema>;

// Resource Limits
export const ResourceLimitsSchema = z.object({
  memory_mb: z.number().optional(),
  cpu_quota: z.number().optional(),
});

export type ResourceLimits = z.infer<typeof ResourceLimitsSchema>;

// Sandbox Config
export const SandboxConfigSchema = z.object({
  image: z.string(),
  name: z.string().optional(),
  port_mappings: z.array(PortMappingSchema),
  environment: z.record(z.string()),
  resource_limits: ResourceLimitsSchema.optional(),
  volumes: z.array(VolumeMountSchema),
  inactivity_timeout_minutes: z.number().optional(),
  pull_policy: z.enum(['always', 'missing', 'never']),
  features: z.array(z.string()),
  enable_all_features: z.boolean(),
  vnc_resolution: z.string().optional(),
});

export type SandboxConfig = z.infer<typeof SandboxConfigSchema>;

// Sandbox Activity
export const SandboxActivitySchema = z.object({
  last_api_activity: z.string().datetime(),
  last_container_activity: z.string().datetime().optional(),
  activity_count: z.number(),
});

export type SandboxActivity = z.infer<typeof SandboxActivitySchema>;

// Kubernetes Info
export const KubernetesInfoSchema = z.object({
  node_name: z.string().optional(),
  pod_ip: z.string().optional(),
  service_name: z.string().optional(),
  message: z.string().optional(),
});

export type KubernetesInfo = z.infer<typeof KubernetesInfoSchema>;

// Sandbox
export const SandboxSchema = z.object({
  id: z.string().uuid(),
  config: SandboxConfigSchema,
  state: SandboxStateSchema,
  container_id: z.string().optional(),
  created_at: z.string().datetime(),
  updated_at: z.string().datetime(),
  error_message: z.string().optional(),
  activity: SandboxActivitySchema,
  deleted_at: z.string().datetime().optional(),
  deleted_by: z.string().optional(),
  kubernetes: KubernetesInfoSchema.optional(),
});

export type Sandbox = z.infer<typeof SandboxSchema>;

// Pagination Meta
export const PaginationMetaSchema = z.object({
  page: z.number(),
  per_page: z.number(),
  total: z.number(),
  total_pages: z.number(),
  has_next: z.boolean(),
  has_prev: z.boolean(),
});

export type PaginationMeta = z.infer<typeof PaginationMetaSchema>;

// Sandbox List Response
export const SandboxListResponseSchema = z.object({
  data: z.array(SandboxSchema),
  pagination: PaginationMetaSchema,
});

export type SandboxListResponse = z.infer<typeof SandboxListResponseSchema>;

// Container Stats
export const ContainerStatsSchema = z.object({
  cpu_percent: z.number(),
  memory_usage_mb: z.number(),
  memory_limit_mb: z.number(),
  memory_percent: z.number(),
  network_rx_bytes: z.number(),
  network_tx_bytes: z.number(),
  block_read_bytes: z.number(),
  block_write_bytes: z.number(),
});

export type ContainerStats = z.infer<typeof ContainerStatsSchema>;

// Image Summary
export const ImageSummarySchema = z.object({
  id: z.string(),
  repo_tags: z.array(z.string()),
  size: z.number(),
  created: z.number(),
  labels: z.record(z.string()).optional(),
});

export type ImageSummary = z.infer<typeof ImageSummarySchema>;

// Progress Detail
export const ProgressDetailSchema = z.object({
  current: z.number(),
  total: z.number(),
});

export type ProgressDetail = z.infer<typeof ProgressDetailSchema>;

// Pull Progress Event
export const PullProgressEventSchema = z.object({
  status: z.string(),
  id: z.string().optional(),
  progress: z.string().optional(),
  progress_detail: ProgressDetailSchema.optional(),
});

export type PullProgressEvent = z.infer<typeof PullProgressEventSchema>;

// Image Details
export const ImageDetailsSchema = z.object({
  id: z.string(),
  repo_tags: z.array(z.string()),
  size: z.number(),
  virtual_size: z.number(),
  created: z.number(),
  architecture: z.string(),
  os: z.string(),
  labels: z.record(z.string()).optional(),
  env: z.array(z.string()).optional(),
  features: z.array(z.string()),
});

export type ImageDetails = z.infer<typeof ImageDetailsSchema>;

// API Response Types
export interface CreateSandboxResponse {
  sandbox: Sandbox;
}

export interface ErrorResponse {
  error: string;
  hint?: string;
}

// Activity Log
export const ActivityLogEntrySchema = z.object({
  id: z.string(),
  sandbox_id: z.string().uuid(),
  activity_type: z.string(),
  message: z.string(),
  timestamp: z.string().datetime(),
  metadata: z.record(z.any()).optional(),
});

export type ActivityLogEntry = z.infer<typeof ActivityLogEntrySchema>;

// Activity Response (from backend activity tracking)
export const ActivityResponseSchema = z.object({
  id: z.string().uuid(),
  sandbox_id: z.string().uuid(),
  activity_type: z.enum([
    'create',
    'delete',
    'exec',
    'stats',
    'stop',
    'cleanup',
    'info',
    'containerActivity',
  ]),
  timestamp: z.string().datetime(),
  details: z.record(z.any()),
});

export type ActivityResponse = z.infer<typeof ActivityResponseSchema>;

// Frontend Configuration (from backend /config endpoint)
export const FrontendConfigSchema = z.object({
  default_sandbox_image: z.string(),
  default_inactivity_timeout: z.number(),
  authentication_required: z.boolean(),
});

export type FrontendConfig = z.infer<typeof FrontendConfigSchema>;
export type ActivityType = ActivityResponse['activity_type'];

// Sandbox Creation Progress Event (from POST /sandboxes/create-stream SSE)
export interface SandboxProgressEvent {
  type: 'pulling' | 'creating' | 'starting' | 'ready' | 'error';
  image?: string;
  status?: string;
  current?: number;
  total?: number;
  sandbox_id?: string;
  container_id?: string;
  message?: string;
}
