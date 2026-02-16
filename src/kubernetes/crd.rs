// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! CRD availability checking utilities

use crate::constants::crd::{POLL_INTERVAL_SECS, POLL_MAX_INTERVAL_SECS};
use crate::error::Result;
use kube::{discovery::Discovery, Client};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

/// Wait for the Cluster CRD to become available in the cluster.
/// This uses exponential backoff starting at POLL_INTERVAL_SECS seconds.
pub async fn wait_for_cluster_crd(client: &Client) -> Result<()> {
    let mut interval = POLL_INTERVAL_SECS;

    loop {
        match check_cluster_crd_exists(client).await {
            Ok(true) => {
                info!("Cluster CRD (provisioning.cattle.io/v1) is available");
                return Ok(());
            }
            Ok(false) => {
                info!(
                    "Cluster CRD (provisioning.cattle.io/v1) not yet available, waiting {} seconds...",
                    interval
                );
            }
            Err(e) => {
                warn!(
                    "Error checking for Cluster CRD: {}, retrying in {} seconds...",
                    e, interval
                );
            }
        }

        sleep(Duration::from_secs(interval)).await;

        // Exponential backoff with max cap
        interval = (interval * 2).min(POLL_MAX_INTERVAL_SECS);
    }
}

/// Check if the Cluster CRD exists by attempting to discover it.
async fn check_cluster_crd_exists(client: &Client) -> Result<bool> {
    let discovery = Discovery::new(client.clone())
        .filter(&["provisioning.cattle.io"])
        .run()
        .await?;

    for group in discovery.groups() {
        if group.name() == "provisioning.cattle.io" {
            for (ar, _) in group.recommended_resources() {
                if ar.kind == "Cluster" && ar.version == "v1" {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}
