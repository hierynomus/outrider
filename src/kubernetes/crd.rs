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
/// Made pub(crate) for testing.
pub(crate) async fn check_cluster_crd_exists(client: &Client) -> Result<bool> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockService;

    fn api_groups_json() -> String {
        // Discovery API lists groups here - include provisioning.cattle.io
        serde_json::json!({
            "kind": "APIGroupList",
            "apiVersion": "v1",
            "groups": [
                {
                    "name": "provisioning.cattle.io",
                    "versions": [
                        {"groupVersion": "provisioning.cattle.io/v1", "version": "v1"}
                    ],
                    "preferredVersion": {"groupVersion": "provisioning.cattle.io/v1", "version": "v1"}
                }
            ]
        })
        .to_string()
    }

    fn api_versions_json() -> String {
        serde_json::json!({
            "kind": "APIVersions",
            "versions": ["v1"]
        })
        .to_string()
    }

    fn provisioning_group_json() -> String {
        serde_json::json!({
            "kind": "APIGroup",
            "apiVersion": "v1",
            "name": "provisioning.cattle.io",
            "versions": [
                {"groupVersion": "provisioning.cattle.io/v1", "version": "v1"}
            ],
            "preferredVersion": {"groupVersion": "provisioning.cattle.io/v1", "version": "v1"}
        })
        .to_string()
    }

    fn provisioning_resources_json() -> String {
        serde_json::json!({
            "kind": "APIResourceList",
            "apiVersion": "v1",
            "groupVersion": "provisioning.cattle.io/v1",
            "resources": [
                {
                    "name": "clusters",
                    "singularName": "cluster",
                    "namespaced": true,
                    "kind": "Cluster",
                    "verbs": ["create", "delete", "get", "list", "patch", "update", "watch"]
                }
            ]
        })
        .to_string()
    }

    #[tokio::test]
    async fn test_crd_exists_returns_true() {
        let mock = MockService::new()
            // Discovery queries these endpoints
            .on_get("/api", 200, &api_versions_json())
            .on_get("/apis", 200, &api_groups_json())
            .on_get("/apis/provisioning.cattle.io", 200, &provisioning_group_json())
            .on_get(
                "/apis/provisioning.cattle.io/v1",
                200,
                &provisioning_resources_json(),
            );

        let client = mock.into_client();
        let result = check_cluster_crd_exists(&client).await;

        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_crd_not_found_returns_false() {
        // When the API group doesn't exist/have the CRD
        let empty_groups = serde_json::json!({
            "kind": "APIGroupList",
            "apiVersion": "v1",
            "groups": []
        })
        .to_string();

        let mock = MockService::new()
            .on_get("/api", 200, &api_versions_json())
            .on_get("/apis", 200, &empty_groups);

        let client = mock.into_client();
        let result = check_cluster_crd_exists(&client).await;

        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
