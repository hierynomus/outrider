// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Central coordinator for syncing secrets to clusters.

use crate::config::Config;
use crate::sync::secrets::{copy_secret_to_cluster, get_enabled_secrets};
use crate::types::cluster::Cluster;
use k8s_openapi::api::core::v1::Secret;
use kube::{api::ListParams, Api, Client, ResourceExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
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
                info!("Cluster '{}' is no longer ready", name);
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
        info!("Cluster became ready, syncing all enabled secrets");

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
