// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0
use anyhow::Result;
use kube::Client;
use tracing::{info, warn};

use outrider::config::Config;
use outrider::kubernetes::wait_for_cluster_crd;
use outrider::reconcilers::{ClusterReconciler, SecretReconciler};
use outrider::sync::SyncManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Outrider operator");

    // Load configuration
    let config = Config::from_env()?;
    info!(
        "Configuration loaded: default_target_namespace={}",
        config.default_target_namespace
    );

    // Create Kubernetes client
    let client = Client::try_default().await?;
    info!("Connected to Kubernetes cluster");

    // Wait for Rancher Cluster CRD before starting reconcilers
    info!("Waiting for Rancher Cluster CRD to become available...");
    wait_for_cluster_crd(&client).await?;

    // Create the sync manager and get a handle for reconcilers
    let (sync_manager, sync_handle) = SyncManager::new(client.clone(), config.clone());

    // Create reconcilers with the sync handle
    let secret_reconciler = SecretReconciler::new(client.clone(), sync_handle.clone());
    let cluster_reconciler = ClusterReconciler::new(client.clone(), sync_handle);

    info!("Starting reconcilers...");

    // Run sync manager and both reconcilers concurrently
    tokio::try_join!(
        sync_manager.run(),
        secret_reconciler.run(),
        cluster_reconciler.run()
    )?;

    // This should never be reached as reconcilers run forever
    warn!("All reconcilers stopped unexpectedly");
    Ok(())
}