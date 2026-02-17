// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Central coordinator for syncing secrets to clusters.

use crate::config::Config;
use crate::sync::secrets::{copy_secret_to_cluster, get_enabled_secrets};
use crate::types::cluster::Cluster;
use k8s_openapi::api::core::v1::Secret;
use kube::{api::ListParams, Api, Client, ResourceExt};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, instrument};

/// Events that reconcilers send to the SyncManager
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// A secret was created or updated
    SecretChanged { secret: Secret },
    /// A cluster became ready
    ClusterBecameReady { cluster: Cluster },
    /// A cluster is no longer ready (no action needed, just logged)
    ClusterBecameNotReady { name: String },
}

/// Central coordinator for syncing secrets to clusters.
/// Receives events from reconcilers and performs the actual sync work.
pub struct SyncManager {
    client: Client,
    config: Config,
    event_rx: mpsc::Receiver<SyncEvent>,
    initial_sync_done: Arc<AtomicBool>,
    /// Tracks clusters that have already received their initial secret sync.
    /// When a cluster becomes ready for the first time (or after being not-ready),
    /// it gets a full sync and is added here. Updates to already-synced clusters
    /// don't trigger re-syncs.
    synced_clusters: Arc<RwLock<HashSet<String>>>,
}

/// Handle to send events to the SyncManager
#[derive(Clone)]
pub struct SyncManagerHandle {
    event_tx: mpsc::Sender<SyncEvent>,
}

impl SyncManagerHandle {
    pub async fn send(&self, event: SyncEvent) {
        if let Err(e) = self.event_tx.send(event).await {
            error!("Failed to send event to SyncManager: {}", e);
        }
    }
}

impl SyncManager {
    pub fn new(client: Client, config: Config) -> (Self, SyncManagerHandle) {
        let (event_tx, event_rx) = mpsc::channel(256);

        let manager = Self {
            client,
            config,
            event_rx,
            initial_sync_done: Arc::new(AtomicBool::new(false)),
            synced_clusters: Arc::new(RwLock::new(HashSet::new())),
        };

        let handle = SyncManagerHandle { event_tx };
        (manager, handle)
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("SyncManager started, performing initial sync...");
        self.initial_sync().await;
        info!("Initial sync complete, listening for events...");

        while let Some(event) = self.event_rx.recv().await {
            self.handle_event(event).await;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn initial_sync(&self) {
        let clusters = match self.get_ready_clusters().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to get ready clusters for initial sync: {}", e);
                return;
            }
        };

        info!("Found {} ready clusters", clusters.len());

        let secrets = match get_enabled_secrets(&self.client).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get enabled secrets for initial sync: {}", e);
                return;
            }
        };

        info!("Found {} enabled secrets", secrets.len());

        for secret in &secrets {
            self.sync_secret_to_clusters(secret, &clusters).await;
        }

        // Mark all ready clusters as synced
        let mut synced = self.synced_clusters.write().await;
        for cluster in &clusters {
            synced.insert(cluster.name_any());
        }

        self.initial_sync_done.store(true, Ordering::SeqCst);
    }

    async fn handle_event(&self, event: SyncEvent) {
        debug!("Handling event: {:?}", event);

        match event {
            SyncEvent::SecretChanged { secret } => {
                self.handle_secret_changed(&secret).await;
            }
            SyncEvent::ClusterBecameReady { cluster } => {
                self.handle_cluster_ready(&cluster).await;
            }
            SyncEvent::ClusterBecameNotReady { name } => {
                self.handle_cluster_not_ready(&name).await;
            }
        }
    }

    #[instrument(skip(self, secret), fields(secret = %format!("{}/{}", secret.namespace().unwrap_or_default(), secret.name_any())))]
    async fn handle_secret_changed(&self, secret: &Secret) {
        // Skip if initial sync hasn't completed yet
        if !self.initial_sync_done.load(Ordering::SeqCst) {
            debug!("Skipping secret change, initial sync not complete");
            return;
        }

        info!("Secret changed, syncing to all ready clusters");

        let clusters = match self.get_ready_clusters().await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to get ready clusters: {}", e);
                return;
            }
        };

        self.sync_secret_to_clusters(secret, &clusters).await;
    }

    #[instrument(skip(self, cluster), fields(cluster = %cluster.name_any()))]
    async fn handle_cluster_ready(&self, cluster: &Cluster) {
        let cluster_name = cluster.name_any();

        // Check if this cluster has already been synced
        if self.synced_clusters.read().await.contains(&cluster_name) {
            debug!(
                "Cluster '{}' already synced, skipping secret sync on update",
                cluster_name
            );
            return;
        }

        info!("New cluster became ready, syncing all enabled secrets");

        let secrets = match get_enabled_secrets(&self.client).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get enabled secrets: {}", e);
                return;
            }
        };

        for secret in &secrets {
            self.sync_secret_to_cluster(secret, cluster).await;
        }

        // Mark this cluster as synced
        self.synced_clusters.write().await.insert(cluster_name);
    }

    #[instrument(skip(self), fields(cluster = %name))]
    async fn handle_cluster_not_ready(&self, name: &str) {
        info!("Cluster '{}' is no longer ready, removing from synced set", name);
        let mut synced = self.synced_clusters.write().await;
        synced.remove(name);
    }

    /// Get all ready Rancher clusters (excluding the local cluster)
    async fn get_ready_clusters(&self) -> crate::error::Result<Vec<Cluster>> {
        let clusters: Api<Cluster> = Api::all(self.client.clone());
        let cluster_list = clusters.list(&ListParams::default()).await?;

        Ok(cluster_list
            .items
            .into_iter()
            .filter(|c| c.is_ready() && !c.is_local())
            .collect())
    }

    async fn sync_secret_to_clusters(&self, secret: &Secret, clusters: &[Cluster]) {
        for cluster in clusters {
            self.sync_secret_to_cluster(secret, cluster).await;
        }
    }

    async fn sync_secret_to_cluster(&self, secret: &Secret, cluster: &Cluster) {
        if let Err(e) = copy_secret_to_cluster(&self.client, secret, cluster, &self.config).await {
            error!(
                "Failed to sync secret {}/{} to cluster {}: {}",
                secret.namespace().unwrap_or_default(),
                secret.name_any(),
                cluster.name_any(),
                e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockService;

    /// Check if a cluster has already been synced
    async fn is_cluster_synced(manager: &SyncManager, cluster_name: &str) -> bool {
        manager.synced_clusters.read().await.contains(cluster_name)
    }

    /// Mark a cluster as synced
    async fn mark_cluster_synced(manager: &SyncManager, cluster_name: &str) {
        manager.synced_clusters.write().await.insert(cluster_name.to_string());
    }

    /// Get the number of synced clusters
    async fn synced_cluster_count(manager: &SyncManager) -> usize {
        manager.synced_clusters.read().await.len()
    }

    #[tokio::test]
    async fn test_synced_clusters_starts_empty() {
        let (manager, _handle) = create_test_manager();
        assert_eq!(synced_cluster_count(&manager).await, 0);
    }

    #[tokio::test]
    async fn test_mark_cluster_synced() {
        let (manager, _handle) = create_test_manager();

        assert!(!is_cluster_synced(&manager, "test-cluster").await);
        mark_cluster_synced(&manager, "test-cluster").await;
        assert!(is_cluster_synced(&manager, "test-cluster").await);
        assert_eq!(synced_cluster_count(&manager).await, 1);
    }

    #[tokio::test]
    async fn test_handle_cluster_not_ready_removes_from_synced() {
        let (manager, _handle) = create_test_manager();

        // Mark cluster as synced first
        mark_cluster_synced(&manager, "test-cluster").await;
        assert!(is_cluster_synced(&manager, "test-cluster").await);

        // Handle not ready event
        manager.handle_cluster_not_ready("test-cluster").await;

        // Cluster should no longer be in synced set
        assert!(!is_cluster_synced(&manager, "test-cluster").await);
        assert_eq!(synced_cluster_count(&manager).await, 0);
    }

    #[tokio::test]
    async fn test_handle_cluster_not_ready_nonexistent_cluster() {
        let (manager, _handle) = create_test_manager();

        // Handle not ready for a cluster that was never synced - should not panic
        manager.handle_cluster_not_ready("nonexistent-cluster").await;
        assert_eq!(synced_cluster_count(&manager).await, 0);
    }

    #[tokio::test]
    async fn test_multiple_clusters_tracked_independently() {
        let (manager, _handle) = create_test_manager();

        mark_cluster_synced(&manager, "cluster-a").await;
        mark_cluster_synced(&manager, "cluster-b").await;
        mark_cluster_synced(&manager, "cluster-c").await;

        assert_eq!(synced_cluster_count(&manager).await, 3);
        assert!(is_cluster_synced(&manager, "cluster-a").await);
        assert!(is_cluster_synced(&manager, "cluster-b").await);
        assert!(is_cluster_synced(&manager, "cluster-c").await);

        // Remove one cluster
        manager.handle_cluster_not_ready("cluster-b").await;

        assert_eq!(synced_cluster_count(&manager).await, 2);
        assert!(is_cluster_synced(&manager, "cluster-a").await);
        assert!(!is_cluster_synced(&manager, "cluster-b").await);
        assert!(is_cluster_synced(&manager, "cluster-c").await);
    }

    #[tokio::test]
    async fn test_cluster_ready_again_after_not_ready() {
        let (manager, _handle) = create_test_manager();

        // Initial sync
        mark_cluster_synced(&manager, "test-cluster").await;
        assert!(is_cluster_synced(&manager, "test-cluster").await);

        // Cluster becomes not ready
        manager.handle_cluster_not_ready("test-cluster").await;
        assert!(!is_cluster_synced(&manager, "test-cluster").await);

        // Cluster becomes ready again - should not be in synced set
        // (in real scenario, handle_cluster_ready would re-sync and add it back)
        assert!(!is_cluster_synced(&manager, "test-cluster").await);
    }

    #[tokio::test]
    async fn test_sync_manager_handle_clone() {
        let (_manager, handle) = create_test_manager();

        // Verify handle can be cloned (needed for passing to multiple reconcilers)
        let _handle2 = handle.clone();
    }

    fn create_test_manager() -> (SyncManager, SyncManagerHandle) {
        let config = Config {
            default_target_namespace: "cattle-global-data".to_string(),
            testing_mode: true,
        };

        let (event_tx, event_rx) = mpsc::channel(256);

        // Use mock client that doesn't require real k8s connection
        let client = MockService::new().into_client();

        let manager = SyncManager {
            client,
            config,
            event_rx,
            initial_sync_done: Arc::new(AtomicBool::new(false)),
            synced_clusters: Arc::new(RwLock::new(HashSet::new())),
        };

        let handle = SyncManagerHandle { event_tx };
        (manager, handle)
    }
}
