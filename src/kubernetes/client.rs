// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Downstream cluster client creation and kubeconfig utilities

use crate::config::Config;
use crate::error::{OutriderError, Result};
use crate::types::cluster::Cluster;
use k8s_openapi::api::core::v1::Secret;
use kube::{config::KubeConfigOptions, Api, Client, Config as KConfig, ResourceExt};
use tracing::{debug, info, instrument};

/// Create a Kubernetes client for a downstream cluster
#[instrument(skip(manager_client, cluster, config), fields(cluster = %cluster.name_any()))]
pub async fn create_downstream_client(
    manager_client: &Client,
    cluster: &Cluster,
    config: &Config,
) -> Result<Client> {
    if config.testing_mode {
        create_testing_client(cluster).await
    } else {
        let kubeconfig = get_cluster_kubeconfig(manager_client, cluster).await?;
        create_client_from_kubeconfig(&kubeconfig).await
    }
}

/// Create a client for testing mode (uses modified cluster URL)
async fn create_testing_client(cluster: &Cluster) -> Result<Client> {
    let mut c = KConfig::infer()
        .await
        .map_err(|e| OutriderError::KubeconfigError(format!("Failed to infer config: {}", e)))?;

    if let Some(cluster_url) = c.cluster_url.to_string().rsplit('/').next() {
        if cluster_url == "local" {
            let new_cluster_url = c
                .cluster_url
                .to_string()
                .replace("local", &cluster.internal_name());
            debug!(
                "Testing mode: modifying cluster URL from {} to {}",
                c.cluster_url, new_cluster_url
            );
            c.cluster_url = new_cluster_url
                .parse()
                .map_err(|e| OutriderError::KubeconfigError(format!("Invalid URL: {}", e)))?;
        }
    }

    Client::try_from(c)
        .map_err(|e| OutriderError::KubeconfigError(format!("Failed to create client: {}", e)))
}

/// Get kubeconfig secret for a Rancher downstream cluster
#[instrument(skip(client, cluster), fields(cluster = %cluster.name_any()))]
async fn get_cluster_kubeconfig(client: &Client, cluster: &Cluster) -> Result<String> {
    let cluster_name = cluster.name_any();
    let secret_name = cluster.kubeconfig_secret_name();
    let namespace = cluster
        .namespace()
        .unwrap_or_else(|| "cattle-system".to_string());
    let secrets: Api<Secret> = Api::namespaced(client.clone(), &namespace);

    info!(
        "Getting kubeconfig secret '{}/{}' for cluster '{}'...",
        namespace, secret_name, cluster_name
    );

    let secret = secrets.get(&secret_name).await.map_err(|e| {
        OutriderError::KubeconfigError(format!(
            "Failed to get kubeconfig secret for cluster {}: {}",
            cluster_name, e
        ))
    })?;

    let Some(data) = secret.data.as_ref() else {
        return Err(OutriderError::KubeconfigError(format!(
            "Kubeconfig secret for cluster {} has no data",
            cluster_name
        )));
    };

    let Some(kubeconfig_data) = data.get("value") else {
        return Err(OutriderError::KubeconfigError(format!(
            "Kubeconfig secret for cluster {} does not contain 'value' key",
            cluster_name
        )));
    };

    String::from_utf8(kubeconfig_data.0.clone()).map_err(|e| {
        OutriderError::KubeconfigError(format!(
            "Failed to decode kubeconfig for cluster {}: {}",
            cluster_name, e
        ))
    })
}

/// Create a Kubernetes client from a kubeconfig string
async fn create_client_from_kubeconfig(kubeconfig: &str) -> Result<Client> {
    use kube::config::Kubeconfig;

    let kubeconfig_parsed: Kubeconfig = serde_yaml::from_str(kubeconfig)
        .map_err(|e| OutriderError::KubeconfigError(format!("Failed to parse kubeconfig: {}", e)))?;

    let client_config =
        kube::Config::from_custom_kubeconfig(kubeconfig_parsed, &KubeConfigOptions::default())
            .await
            .map_err(|e| {
                OutriderError::KubeconfigError(format!("Failed to create config: {}", e))
            })?;

    Client::try_from(client_config)
        .map_err(|e| OutriderError::KubeconfigError(format!("Failed to create client: {}", e)))
}
