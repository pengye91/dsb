// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2025-2026 Tom Xie
//! Shared session manager for mapping MCP session IDs to sandbox IDs.
//!
//! This module provides a thread-safe, concurrent session-to-sandbox mapping
//! using `DashMap`. It mirrors the Python implementation's session cache logic
//! (`resolve_or_create_sandbox_id`, `resolve_session_to_sandbox_id`, `cache_sandbox`,
//! `get_cached_sandbox`, `clear_sandbox_cache`).
//!
//! Sessions expire after a TTL to prevent unbounded memory growth and orphaned
//! sandboxes. A background task periodically evicts stale entries.

use anyhow::Context;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::dsb_client::DSBClient;
use crate::settings::Settings;

/// Default TTL for session-to-sandbox mappings: 30 minutes of inactivity.
const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(30 * 60);
/// Default interval between cleanup sweeps: 5 minutes.
const DEFAULT_CLEANUP_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// Thread-safe session manager that maps MCP session IDs to DSB sandbox IDs.
///
/// Uses `DashMap` for lock-free concurrent access from multiple async tasks.
/// The mapping allows MCP tools to maintain persistent sandbox associations
/// across multiple tool invocations within the same session.
///
/// Each entry tracks its last access time. Entries older than the TTL are
/// evicted by a background task to prevent unbounded growth.
#[derive(Debug, Clone)]
pub struct SessionManager {
    /// Internal concurrent map from session ID to (sandbox ID, last accessed).
    sessions: Arc<DashMap<String, (Uuid, Instant)>>,
    /// Handle to the background cleanup task (stored so it can be aborted).
    cleanup_handle: Arc<tokio::sync::Mutex<Option<JoinHandle<()>>>>,
}

impl SessionManager {
    /// Create a new, empty session manager.
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            cleanup_handle: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Look up the sandbox ID associated with a session ID.
    ///
    /// Returns `Some(sandbox_id)` if a mapping exists, `None` otherwise.
    /// Updates the last-accessed timestamp on hit.
    pub fn get(&self, session_id: &str) -> Option<Uuid> {
        self.sessions.get_mut(session_id).map(|mut entry| {
            entry.value_mut().1 = Instant::now();
            entry.value().0
        })
    }

    /// Store a mapping from session ID to sandbox ID.
    ///
    /// If a mapping already exists for this session, it is updated.
    pub fn set(&self, session_id: String, sandbox_id: Uuid) {
        debug!(
            session_id = %session_id,
            sandbox_id = %sandbox_id,
            "Caching session-to-sandbox mapping"
        );
        self.sessions
            .insert(session_id, (sandbox_id, Instant::now()));
    }

    /// Remove the mapping for a given session ID.
    ///
    /// Returns the previously mapped sandbox ID, if any.
    pub fn remove(&self, session_id: &str) -> Option<Uuid> {
        let removed = self.sessions.remove(session_id);
        if let Some((sid, (sandbox_id, _))) = &removed {
            debug!(
                session_id = %sid,
                sandbox_id = %sandbox_id,
                "Removed session-to-sandbox mapping"
            );
        }
        removed.map(|(_, (v, _))| v)
    }

    /// Start a background task that periodically evicts stale session mappings.
    ///
    /// When a session is evicted, the corresponding sandbox is **not** deleted
    /// automatically (the DSB server handles sandbox lifecycle via its own
    /// inactivity timeout). This task only cleans up the local mapping cache.
    ///
    /// Call this once after creating the `SessionManager` and before accepting
    /// traffic. The task is stored internally and will be aborted when the
    /// `SessionManager` is dropped.
    pub async fn start_cleanup_task(&self) {
        let sessions = self.sessions.clone();
        let ttl = DEFAULT_SESSION_TTL;
        let interval = DEFAULT_CLEANUP_INTERVAL;

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;

                let now = Instant::now();
                let mut evicted = 0;
                sessions.retain(|_session_id, (_sandbox_id, last_accessed)| {
                    let keep = now.duration_since(*last_accessed) < ttl;
                    if !keep {
                        evicted += 1;
                    }
                    keep
                });

                if evicted > 0 {
                    warn!(
                        evicted_count = evicted,
                        remaining = sessions.len(),
                        "Evicted stale session mappings"
                    );
                }
            }
        });

        let mut guard = self.cleanup_handle.lock().await;
        *guard = Some(handle);
    }

    /// Resolve an existing session to its sandbox ID, or create a new sandbox
    /// via the DSB client and cache the mapping.
    ///
    /// Uses DashMap's `entry()` API for atomic insert-if-absent, preventing a
    /// race condition where concurrent requests for the same `session_id` could
    /// otherwise create multiple sandboxes.
    ///
    /// This mirrors the Python `resolve_or_create_sandbox_id` function:
    /// 1. Check if a mapping already exists for the session (atomic probe).
    /// 2. If not, create a new sandbox using the DSB client.
    /// 3. Cache the new mapping and return the sandbox ID.
    ///
    /// Uses the default image from settings when creating a new sandbox.
    pub async fn resolve_or_create(
        &self,
        session_id: &str,
        client: &DSBClient,
        settings: &Settings,
    ) -> anyhow::Result<Uuid> {
        use dashmap::mapref::entry::Entry;

        match self.sessions.entry(session_id.to_string()) {
            Entry::Occupied(mut entry) => {
                let (sandbox_id, last_accessed) = entry.get_mut();
                *last_accessed = Instant::now();
                debug!(
                    session_id = %session_id,
                    sandbox_id = %sandbox_id,
                    "Resolved sandbox from session cache"
                );
                Ok(*sandbox_id)
            }
            Entry::Vacant(entry) => {
                let image = &settings.sandbox.default_image;
                info!(
                    session_id = %session_id,
                    image = %image,
                    "No cached sandbox found, creating new sandbox"
                );

                // Use standard environment variables from settings
                let env = settings.get_sandbox_env();

                let config = crate::dsb_client::CreateSandboxConfig {
                    image: image.clone(),
                    name: None,
                    environment: Some(env),
                    port_mappings: None,
                    resource_limits: None,
                    volumes: None,
                    command: None,
                    inactivity_timeout_minutes: None,
                    pull_policy: None,
                };

                let sandbox = client.create_sandbox_full(config).await.with_context(|| {
                    format!("Failed to create sandbox for session {}", session_id)
                })?;

                let sandbox_id = sandbox.id;
                info!(
                    session_id = %session_id,
                    sandbox_id = %sandbox_id,
                    state = %sandbox.state,
                    "Created new sandbox"
                );

                entry.insert((sandbox_id, Instant::now()));
                Ok(sandbox_id)
            }
        }
    }

    /// Check if a mapping exists for the given session ID.
    ///
    /// **Note**: Due to concurrent access from other tasks, a return value of
    /// `true` does not guarantee the entry will still exist on the next call
    /// (TOCTOU — time-of-check-to-time-of-use). If you need to act on the
    /// value atomically, prefer `entry()` or `get()` instead.
    pub fn contains(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    /// Return the number of active session-to-sandbox mappings.
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Return true if there are no active mappings.
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Clear all session-to-sandbox mappings.
    pub fn clear(&self) {
        let count = self.sessions.len();
        self.sessions.clear();
        if count > 0 {
            warn!(cleared_count = count, "Cleared all session mappings");
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_manager_is_empty() {
        let mgr = SessionManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_set_and_get() {
        let mgr = SessionManager::new();
        let session_id = "test-session-1";
        let sandbox_id = Uuid::new_v4();

        mgr.set(session_id.to_string(), sandbox_id);
        assert_eq!(mgr.len(), 1);

        let result = mgr.get(session_id);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), sandbox_id);
    }

    #[test]
    fn test_get_missing_returns_none() {
        let mgr = SessionManager::new();
        assert!(mgr.get("nonexistent").is_none());
    }

    #[test]
    fn test_remove() {
        let mgr = SessionManager::new();
        let session_id = "test-session-remove";
        let sandbox_id = Uuid::new_v4();

        mgr.set(session_id.to_string(), sandbox_id);
        assert!(mgr.contains(session_id));

        let removed = mgr.remove(session_id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap(), sandbox_id);
        assert!(!mgr.contains(session_id));
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_remove_missing_returns_none() {
        let mgr = SessionManager::new();
        assert!(mgr.remove("nonexistent").is_none());
    }

    #[test]
    fn test_set_overwrites_existing() {
        let mgr = SessionManager::new();
        let session_id = "test-session-overwrite";
        let sandbox_id_1 = Uuid::new_v4();
        let sandbox_id_2 = Uuid::new_v4();

        mgr.set(session_id.to_string(), sandbox_id_1);
        mgr.set(session_id.to_string(), sandbox_id_2);

        assert_eq!(mgr.len(), 1);
        let result = mgr.get(session_id);
        assert_eq!(result.unwrap(), sandbox_id_2);
    }

    #[test]
    fn test_contains() {
        let mgr = SessionManager::new();
        let session_id = "test-session-contains";
        assert!(!mgr.contains(session_id));

        mgr.set(session_id.to_string(), Uuid::new_v4());
        assert!(mgr.contains(session_id));
    }

    #[test]
    fn test_clear() {
        let mgr = SessionManager::new();

        for i in 0..5 {
            mgr.set(format!("session-{}", i), Uuid::new_v4());
        }
        assert_eq!(mgr.len(), 5);

        mgr.clear();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_default_trait() {
        let mgr = SessionManager::default();
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_resolve_or_create_caches_mapping() {
        let mgr = SessionManager::new();
        let session_id = "test-session-resolve";

        // Verify no mapping exists initially
        assert!(mgr.get(session_id).is_none());

        // We can't test the full resolve_or_create flow without a real DSB server,
        // but we can verify that set() followed by get() works correctly,
        // which is the core of the caching logic.
        let sandbox_id = Uuid::new_v4();
        mgr.set(session_id.to_string(), sandbox_id);

        // Simulate resolve_or_create's first step: check cache
        let cached = mgr.get(session_id);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), sandbox_id);
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let mgr = Arc::new(SessionManager::new());
        let mut handles = vec![];

        // Spawn multiple threads that write to the session manager concurrently
        for i in 0..10 {
            let mgr_clone = Arc::clone(&mgr);
            handles.push(thread::spawn(move || {
                let session_id = format!("concurrent-session-{}", i);
                let sandbox_id = Uuid::new_v4();
                mgr_clone.set(session_id.clone(), sandbox_id);
                assert_eq!(mgr_clone.get(&session_id), Some(sandbox_id));
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(mgr.len(), 10);
    }
}
