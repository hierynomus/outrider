use crate::config::Config;
use crate::controllers::utils::{copy_secret_to_cluster, get_enabled_secrets, is_cluster_ready, is_local_cluster};
use crate::error::{OutriderError, Result};
use crate::types::cluster::Cluster;
use futures::StreamExt;
use kube::{
    runtime::{controller::Action, watcher, Controller},
    Api, Client, ResourceExt,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub struct ClusterReconciler {
    client: Client,
    config: Config,
}

impl ClusterReconciler {
    pub fn new(client: Client, config: Config) -> Self {
        Self { client, config }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let clusters: Api<Cluster> = Api::all(self.client.clone());

        let context = Arc::new(self);

        Controller::new(clusters, watcher::Config::default())
            .run(reconcile, error_policy, context)
            .for_each(|res| async move {
                match res {
                    Ok(o) => debug!("Reconciled cluster: {:?}", o),
                    Err(e) => warn!("Reconciliation error: {:?}", e),
                }
            })
            .await;

        Ok(())
    }
}

async fn reconcile(cluster: Arc<Cluster>, ctx: Arc<ClusterReconciler>) -> Result<Action> {
    let name = cluster.name_any();

    if is_local_cluster(&cluster) {
        debug!("Skipping local cluster");
        return Ok(Action::await_change());
    }

    debug!("Reconciling cluster: {}", name);

    // Check if cluster is ready
    if !is_cluster_ready(&cluster) {
        debug!("Cluster '{}' is not ready yet, skipping", name);
        return Ok(Action::requeue(Duration::from_secs(30)));
    }

    info!("Cluster '{}' is ready, copying secrets", name);

    // Get all enabled secrets
    let secrets = match get_enabled_secrets(&ctx.client).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to get enabled secrets: {}", e);
            return Err(e);
        }
    };

    if secrets.is_empty() {
        info!("No enabled secrets found, nothing to do");
        return Ok(Action::await_change());
    }

    info!("Found {} enabled secrets", secrets.len());

    // Copy all enabled secrets to this cluster
    for secret in secrets {
        let secret_name = secret.name_any();
        let secret_ns = secret.namespace().unwrap_or_default();

        match copy_secret_to_cluster(&ctx.client, &secret, &cluster, &ctx.config).await {
            Ok(_) => {
                info!(
                    "Successfully copied secret {}/{} to cluster {}",
                    secret_ns, secret_name, name
                );
            }
            Err(e) => {
                error!(
                    "Failed to copy secret {}/{} to cluster {}: {}",
                    secret_ns, secret_name, name, e
                );
                // Continue with other secrets even if one fails
            }
        }
    }

    // Recheck after 5 minutes in case new secrets appear
    Ok(Action::requeue(Duration::from_secs(300)))
}

fn error_policy(
    _cluster: Arc<Cluster>,
    error: &OutriderError,
    _ctx: Arc<ClusterReconciler>,
) -> Action {
    error!("Reconciliation error: {}", error);
    Action::requeue(Duration::from_secs(60))
}