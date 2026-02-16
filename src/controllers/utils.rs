// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0
use crate::config::Config;
use crate::error::{OutriderError, Result};
use crate::types::cluster::Cluster;
use k8s_openapi::api::core::v1::{Namespace, Secret};
use kube::{
    api::{ListParams, ObjectMeta, Patch, PatchParams, PostParams},
    discovery::Discovery,
    Api, Client, ResourceExt,
    Config as KConfig,
};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

const CRD_POLL_INTERVAL_SECS: u64 = 10;
const CRD_POLL_MAX_INTERVAL_SECS: u64 = 60;

/// Wait for the Cluster CRD to become available in the cluster.
/// This uses exponential backoff starting at CRD_POLL_INTERVAL_SECS seconds.
pub async fn wait_for_cluster_crd(client: &Client) -> Result<()> {
    let mut interval = CRD_POLL_INTERVAL_SECS;

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
        interval = (interval * 2).min(CRD_POLL_MAX_INTERVAL_SECS);
    }
}

/// Check if the Cluster CRD exists by attempting to discover it.
async fn check_cluster_crd_exists(client: &Client) -> Result<bool> {
    // Try to discover the provisioning.cattle.io API group
    let discovery = Discovery::new(client.clone())
        .filter(&["provisioning.cattle.io"])
        .run()
        .await?;

    // Check if the Cluster resource exists in the v1 version
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

const ENABLED_ANNOTATION: &str = "outrider.geeko.me/enabled";
const NAMESPACE_ANNOTATION: &str = "outrider.geeko.me/namespace";

/// Get all secrets that have the enabled annotation
pub async fn get_enabled_secrets(client: &Client) -> Result<Vec<Secret>> {
    let secrets: Api<Secret> = Api::all(client.clone());
    let lp = ListParams::default();

    let secret_list = secrets.list(&lp).await?;

    Ok(secret_list
        .items
        .into_iter()
        .filter(|s| {
            s.metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get(ENABLED_ANNOTATION))
                .map(|v| v == "true")
                .unwrap_or(false)
        })
        .collect())
}

/// Get all ready Rancher clusters
pub async fn get_ready_clusters(client: &Client) -> Result<Vec<Cluster>> {
    let clusters: Api<Cluster> = Api::all(client.clone());
    let lp = ListParams::default();

    let cluster_list = clusters.list(&lp).await?;

    Ok(cluster_list
        .items
        .into_iter()
        .filter(|c| is_cluster_ready(c) && !is_local_cluster(c))
        .collect())
}

pub fn is_local_cluster(cluster: &Cluster) -> bool {
    cluster.name_any() == "local"
}

/// Check if a Rancher cluster is ready
pub fn is_cluster_ready(cluster: &Cluster) -> bool {
    // Check status.conditions for Ready=True
    cluster
        .status
        .as_ref()
        .and_then(|s| s.conditions.as_ref())
        .map(|conditions: &Vec<_>| {
            conditions.iter().any(|condition| {
                condition.condition_type == "Ready" && condition.status == "True"
            })
        })
        .unwrap_or(false)
}

/// Copy a secret to a downstream cluster
pub async fn copy_secret_to_cluster(
    manager_client: &Client,
    secret: &Secret,
    cluster: &Cluster,
    config: &Config,
) -> Result<()> {
    let cluster_name = cluster.name_any();
    let secret_name = secret.name_any();
    let source_namespace = secret.namespace().unwrap_or_default();

    info!(
        "Copying secret {}/{} to cluster {}",
        source_namespace, secret_name, cluster_name
    );

    // Determine target namespace
    let target_namespace = secret
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(NAMESPACE_ANNOTATION))
        .map(|s| s.as_str())
        .unwrap_or(&config.default_target_namespace);

    debug!("Target namespace: {}", target_namespace);

    let downstream_client: Client;
    if config.testing_mode {
        let mut c = KConfig::infer().await.unwrap();
        // Replace the last part of the cluster_url if it is "local" and replace it with the cluster name
        if let Some(cluster_url) = c.cluster_url.to_string().rsplit('/').next() {
            if cluster_url == "local" {
                let new_cluster_url = c.cluster_url.to_string().replace("local", &cluster.status.as_ref().map(|s|s.cluster_name.clone()).unwrap_or_else(|| cluster_name.clone()));
                debug!("Testing mode: modifying cluster URL from {} to {}", c.cluster_url, new_cluster_url);
                c.cluster_url = new_cluster_url.parse().unwrap();
            }
        }
        downstream_client = Client::try_from(c).unwrap();
    } else {

        // Get kubeconfig for downstream cluster
        let kubeconfig = get_cluster_kubeconfig(manager_client, &cluster).await?;

        // Create client for downstream cluster
        downstream_client = create_client_from_kubeconfig(&kubeconfig).await?;
    }

    // Ensure target namespace exists in downstream cluster
    ensure_namespace_exists(&downstream_client, target_namespace).await?;

    // Copy the secret to downstream cluster
    let downstream_secrets: Api<Secret> = Api::namespaced(downstream_client, target_namespace);

    let new_secret = create_downstream_secret(secret, target_namespace);

    // Apply the secret (create or update)
    let pp = PatchParams::apply("outrider").force();
    let patch = Patch::Apply(&new_secret);

    downstream_secrets
        .patch(&secret_name, &pp, &patch)
        .await?;

    info!(
        "Successfully copied secret {}/{} to cluster {}/{}",
        source_namespace, secret_name, cluster_name, target_namespace
    );

    Ok(())
}

fn create_downstream_secret(secret: &Secret, target_namespace: &str) -> Secret {
    // Filter out outrider annotations from the copied secret
    let filtered_annotations = secret
        .metadata
        .annotations
        .as_ref()
        .map(|a| {
            a.iter()
                .filter(|(k, _)| !k.starts_with("outrider.geeko.me/"))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        });

    let mut downstream_secret = secret.clone();
    downstream_secret.metadata = ObjectMeta {
        name: secret.metadata.name.clone(),
        namespace: Some(target_namespace.to_string()),
        labels: secret.metadata.labels.clone(),
        annotations: filtered_annotations,
        ..Default::default()
    };
    downstream_secret
}

/// Get kubeconfig secret for a Rancher downstream cluster
async fn get_cluster_kubeconfig(client: &Client, cluster: &Cluster) -> Result<String> {
    let cluster_name = cluster.name_any();
    let secret_name: String = cluster.status.as_ref().and_then(|s| Some(s.client_secret_name.clone())).get_or_insert_default().clone();
    let namespace = cluster.namespace().unwrap_or_else(|| "cattle-system".to_string());
    let secrets: Api<Secret> = Api::namespaced(client.clone(), &namespace);

    info!("Getting kubeconfig secret '{}/{}' for cluster '{}'...", namespace, secret_name, cluster_name);

    let secret = secrets.get(&secret_name).await.map_err(|e| {
        OutriderError::KubeconfigError(format!(
            "Failed to get kubeconfig secret for cluster {}: {}",
            cluster_name, e
        ))
    })?;

    let kubeconfig_data = secret
        .data
        .as_ref()
        .and_then(|d| d.get("value"))
        .ok_or_else(|| {
            OutriderError::KubeconfigError(format!(
                "Kubeconfig secret for cluster {} does not contain 'value' key",
                cluster_name
            ))
        })?;

    let kubeconfig = String::from_utf8(kubeconfig_data.0.clone()).map_err(|e| {
        OutriderError::KubeconfigError(format!(
            "Failed to decode kubeconfig for cluster {}: {}",
            cluster_name, e
        ))
    })?;

    Ok(kubeconfig)
}

/// Create a Kubernetes client from a kubeconfig string
async fn create_client_from_kubeconfig(kubeconfig: &str) -> Result<Client> {
    use kube::config::{Kubeconfig, KubeConfigOptions};

    info!("Creating Kubernetes client from kubeconfig for cluster: {}...", kubeconfig);

    let kubeconfig_parsed: Kubeconfig = serde_yaml::from_str(kubeconfig).map_err(|e| {
        OutriderError::KubeconfigError(format!("Failed to parse kubeconfig: {}", e))
    })?;

    info!("Creating Kubernetes client from kubeconfig {:?}...", kubeconfig_parsed);

    let client_config = kube::Config::from_custom_kubeconfig(
        kubeconfig_parsed,
        &KubeConfigOptions::default(),
    )
    .await
    .map_err(|e| OutriderError::KubeconfigError(format!("Failed to create config: {}", e)))?;

    let client = Client::try_from(client_config)
        .map_err(|e| OutriderError::KubeconfigError(format!("Failed to create client: {}", e)))?;

    Ok(client)
}

/// Ensure a namespace exists in the cluster, create if it doesn't
async fn ensure_namespace_exists(client: &Client, namespace: &str) -> Result<()> {
    let namespaces: Api<Namespace> = Api::all(client.clone());

    match namespaces.get(namespace).await {
        Ok(_) => {
            debug!("Namespace {} already exists", namespace);
            Ok(())
        }
        Err(kube::Error::Api(err)) if err.code == 404 => {
            info!("Creating namespace {}", namespace);
            let ns = Namespace {
                metadata: ObjectMeta {
                    name: Some(namespace.to_string()),
                    ..Default::default()
                },
                ..Default::default()
            };
            namespaces.create(&PostParams::default(), &ns).await?;
            info!("Namespace {} created successfully", namespace);
            Ok(())
        }
        Err(e) => Err(OutriderError::NamespaceError(format!(
            "Failed to check/create namespace {}: {}",
            namespace, e
        ))),
    }
}