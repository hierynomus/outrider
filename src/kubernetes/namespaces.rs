// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Namespace management utilities

use crate::error::{OutriderError, Result};
use k8s_openapi::api::core::v1::Namespace;
use kube::{
    api::{ObjectMeta, PostParams},
    Api, Client,
};
use tracing::{debug, info, instrument};

/// Ensure a namespace exists in the cluster, create if it doesn't
#[instrument(skip(client))]
pub async fn ensure_namespace_exists(client: &Client, namespace: &str) -> Result<()> {
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
