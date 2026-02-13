// Copyright 2025, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0
use crate::config::Config;
use crate::controllers::utils::{copy_secret_to_cluster, get_ready_clusters};
use crate::error::{OutriderError, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    runtime::{controller::Action, Controller},
    Api, Client, ResourceExt,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use kube_runtime::watcher::{Config as WatcherConfig};

const ENABLED_ANNOTATION: &str = "outrider.geeko.me/enabled";

pub struct SecretReconciler {
    client: Client,
    config: Config,
}

impl SecretReconciler {
    pub fn new(client: Client, config: Config) -> Self {
        Self { client, config }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let secrets: Api<Secret> = Api::all(self.client.clone());
        let context = Arc::new(self);

        Controller::new(secrets, WatcherConfig::default())
            .run(reconcile, error_policy, context)
            .for_each(|res| async move {
                match res {
                    Ok(o) => debug!("Reconciled secret: {:?}", o),
                    Err(e) => warn!("Reconciliation error: {:?}", e),
                }
            })
            .await;

        Ok(())
    }
}

async fn reconcile(secret: Arc<Secret>, ctx: Arc<SecretReconciler>) -> Result<Action> {
    let name = secret.name_any();
    let namespace = secret.namespace().unwrap_or_default();

    debug!("Reconciling secret: {}/{}", namespace, name);

    // Check if secret has the enabled annotation
    let annotations = secret.metadata.annotations.as_ref();
    let is_enabled = annotations
        .and_then(|a| a.get(ENABLED_ANNOTATION))
        .map(|v| v == "true")
        .unwrap_or(false);

    if !is_enabled {
        debug!(
            "Secret {}/{} does not have enabled annotation, skipping",
            namespace, name
        );
        return Ok(Action::await_change());
    }

    info!(
        "Processing enabled secret: {}/{}",
        namespace, name
    );

    // Get all ready clusters
    let clusters = match get_ready_clusters(&ctx.client).await {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to get ready clusters: {}", e);
            return Err(e);
        }
    };

    if clusters.is_empty() {
        info!("No ready clusters found, nothing to do");
        return Ok(Action::requeue(Duration::from_secs(60)));
    }

    info!("Found {} ready clusters", clusters.len());

    // Copy secret to all ready clusters
    for cluster in clusters {
        let cluster_name = cluster.name_any();
        match copy_secret_to_cluster(&ctx.client, &secret, &cluster, &ctx.config).await {
            Ok(_) => {
                info!(
                    "Successfully copied secret {}/{} to cluster {}",
                    namespace, name, cluster_name
                );
            }
            Err(e) => {
                error!(
                    "Failed to copy secret {}/{} to cluster {}: {}",
                    namespace, name, cluster_name, e
                );
                // Continue with other clusters even if one fails
            }
        }
    }

    // Recheck after 5 minutes in case new clusters appear
    Ok(Action::requeue(Duration::from_secs(300)))
}

fn error_policy(_secret: Arc<Secret>, error: &OutriderError, _ctx: Arc<SecretReconciler>) -> Action {
    error!("Reconciliation error: {}", error);
    Action::requeue(Duration::from_secs(60))
}