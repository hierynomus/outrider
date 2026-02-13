// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0
use crate::config::Config;
use crate::controllers::utils::{copy_secret_to_cluster, get_enabled_secrets, get_ready_clusters};
use crate::types::cluster::Cluster;
use k8s_openapi::api::core::v1::Secret;
use kube::{Client, ResourceExt};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

/// Events that controllers send to the SyncManager
#[derive(Debug, Clone)]
pub enum SyncEvent {
    /// A secret was created or updated
    SecretChanged { secret: Secret },
    /// A cluster became ready
    ClusterBecameReady { cluster: Cluster },
    /// A cluster is no longer ready
    ClusterBecameNotReady { name: String },
}

/// Central coordinator for syncing secrets to clusters.
/// Receives events from controllers and performs the actual sync work.
pub struct SyncManager {
    client: Client,
    config: Config,
    /// Receiver for sync events from controllers
    event_rx: mpsc::Receiver<SyncEvent>,
    /// Tracks which clusters are currently ready
    ready_clusters: Arc<RwLock<HashSet<String>>>,
    /// Tracks secrets we've already synced on startup (to avoid double sync)
    initial_sync_done: Arc<RwLock<bool>>,
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
            ready_clusters: Arc::new(RwLock::new(HashSet::new())),
            initial_sync_done: Arc::new(RwLock::new(false)),
        };

        let handle = SyncManagerHandle { event_tx };

        (manager, handle)
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        info!("SyncManager started, performing initial sync...");

        // Perform initial sync: get all enabled secrets and ready clusters
        self.initial_sync().await;

        info!("Initial sync complete, listening for events...");

        // Process events from controllers
        while let Some(event) = self.event_rx.recv().await {
            self.handle_event(event).await;
        }

        Ok(())
    }

    async fn initial_sync(&self) {
        // Get all ready clusters and track them
        let clusters = match get_ready_clusters(&self.client).await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to get ready clusters for initial sync: {}", e);
                return;
            }
        };

        // Track ready clusters
        {
            let mut ready = self.ready_clusters.write().await;
            for cluster in &clusters {
                ready.insert(cluster.name_any());
            }
        }

        info!("Found {} ready clusters", clusters.len());

        // Get all enabled secrets
        let secrets = match get_enabled_secrets(&self.client).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get enabled secrets for initial sync: {}", e);
                return;
            }
        };

        info!("Found {} enabled secrets", secrets.len());

        // Sync all secrets to all ready clusters
        for secret in &secrets {
            self.sync_secret_to_all_clusters(secret, &clusters).await;
        }

        // Mark initial sync as done
        *self.initial_sync_done.write().await = true;
    }

    async fn handle_event(&self, event: SyncEvent) {
        debug!("Handling event: {:?}", event);

        match event {
            SyncEvent::SecretChanged { secret } => {
                self.handle_secret_changed(secret).await;
            }
            SyncEvent::ClusterBecameReady { cluster } => {
                self.handle_cluster_ready(cluster).await;
            }
            SyncEvent::ClusterBecameNotReady { name } => {
                self.handle_cluster_not_ready(&name).await;
            }
        }
    }

    async fn handle_secret_changed(&self, secret: Secret) {
        let name = secret.name_any();
        let namespace = secret.namespace().unwrap_or_default();

        // Skip if initial sync hasn't completed yet (we'll sync everything there)
        if !*self.initial_sync_done.read().await {
            debug!(
                "Skipping secret {}/{} change, initial sync not complete",
                namespace, name
            );
            return;
        }

        info!("Secret {}/{} changed, syncing to all ready clusters", namespace, name);

        // Get ready clusters
        let clusters = match get_ready_clusters(&self.client).await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to get ready clusters: {}", e);
                return;
            }
        };

        self.sync_secret_to_all_clusters(&secret, &clusters).await;
    }

    async fn handle_cluster_ready(&self, cluster: Cluster) {
        let cluster_name = cluster.name_any();

        // Check if we already knew about this cluster
        let was_known = self.ready_clusters.read().await.contains(&cluster_name);

        if was_known {
            debug!("Cluster '{}' was already known as ready, skipping", cluster_name);
            return;
        }

        info!("Cluster '{}' became ready, syncing all enabled secrets", cluster_name);

        // Add to ready set
        self.ready_clusters.write().await.insert(cluster_name.clone());

        // Get all enabled secrets
        let secrets = match get_enabled_secrets(&self.client).await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get enabled secrets: {}", e);
                return;
            }
        };

        // Sync all secrets to this cluster
        for secret in &secrets {
            self.sync_secret_to_cluster(secret, &cluster).await;
        }
    }

    async fn handle_cluster_not_ready(&self, cluster_name: &str) {
        info!("Cluster '{}' is no longer ready", cluster_name);
        self.ready_clusters.write().await.remove(cluster_name);
    }

    async fn sync_secret_to_all_clusters(&self, secret: &Secret, clusters: &[Cluster]) {
        for cluster in clusters {
            self.sync_secret_to_cluster(secret, cluster).await;
        }
    }

    async fn sync_secret_to_cluster(&self, secret: &Secret, cluster: &Cluster) {
        let secret_name = secret.name_any();
        let secret_ns = secret.namespace().unwrap_or_default();
        let cluster_name = cluster.name_any();

        match copy_secret_to_cluster(&self.client, secret, cluster, &self.config).await {
            Ok(_) => {
                info!(
                    "Successfully synced secret {}/{} to cluster {}",
                    secret_ns, secret_name, cluster_name
                );
            }
            Err(e) => {
                error!(
                    "Failed to sync secret {}/{} to cluster {}: {}",
                    secret_ns, secret_name, cluster_name, e
                );
            }
        }
    }
}
