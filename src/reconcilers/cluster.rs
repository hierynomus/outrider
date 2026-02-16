// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Cluster reconciler - watches Rancher Cluster resources and notifies sync manager.

use crate::error::{OutriderError, Result};
use crate::sync::{SyncEvent, SyncManagerHandle};
use crate::types::cluster::Cluster;
use futures::StreamExt;
use kube::{
    runtime::{controller::Action, watcher, Controller},
    Api, Client, ResourceExt,
};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

pub struct ClusterReconciler {
    client: Client,
    sync_handle: SyncManagerHandle,
}

impl ClusterReconciler {
    pub fn new(client: Client, sync_handle: SyncManagerHandle) -> Self {
        Self { client, sync_handle }
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

    if cluster.is_local() {
        debug!("Skipping local cluster");
        return Ok(Action::await_change());
    }

    debug!("Reconciling cluster: {}", name);

    // Notify the sync manager about the cluster state
    if cluster.is_ready() {
        ctx.sync_handle
            .send(SyncEvent::ClusterBecameReady {
                cluster: (*cluster).clone(),
            })
            .await;
    } else {
        ctx.sync_handle
            .send(SyncEvent::ClusterBecameNotReady { name: name.clone() })
            .await;
    }

    // Wait for the next change - the watcher will notify us when the cluster changes or is deleted
    Ok(Action::await_change())
}

fn error_policy(
    _cluster: Arc<Cluster>,
    error: &OutriderError,
    _ctx: Arc<ClusterReconciler>,
) -> Action {
    error!("Reconciliation error: {}", error);
    Action::requeue(Duration::from_secs(60))
}
