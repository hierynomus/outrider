// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Secret reconciler - watches Secrets and notifies sync manager of enabled ones.

use crate::constants::annotations;
use crate::error::{OutriderError, Result};
use crate::sync::{SyncEvent, SyncManagerHandle};
use futures::StreamExt;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    runtime::{controller::Action, Controller},
    Api, Client, ResourceExt,
};
use kube_runtime::watcher::Config as WatcherConfig;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, warn};

pub struct SecretReconciler {
    client: Client,
    sync_handle: SyncManagerHandle,
}

impl SecretReconciler {
    pub fn new(client: Client, sync_handle: SyncManagerHandle) -> Self {
        Self { client, sync_handle }
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
    let is_enabled = secret
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(annotations::ENABLED))
        .is_some_and(|v| v == "true");

    if !is_enabled {
        debug!(
            "Secret {}/{} does not have enabled annotation, skipping",
            namespace, name
        );
        return Ok(Action::await_change());
    }

    // Notify the sync manager about the secret change
    ctx.sync_handle
        .send(SyncEvent::SecretChanged {
            secret: (*secret).clone(),
        })
        .await;

    Ok(Action::await_change())
}

fn error_policy(
    _secret: Arc<Secret>,
    error: &OutriderError,
    _ctx: Arc<SecretReconciler>,
) -> Action {
    error!("Reconciliation error: {}", error);
    Action::requeue(Duration::from_secs(60))
}
