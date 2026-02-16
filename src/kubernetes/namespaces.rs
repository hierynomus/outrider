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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{namespace_json, MockService};

    #[tokio::test]
    async fn test_namespace_already_exists() {
        let mock = MockService::new()
            .on_get("/api/v1/namespaces/test-ns", 200, &namespace_json("test-ns"));

        let client = mock.into_client();
        let result = ensure_namespace_exists(&client, "test-ns").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_namespace_created_when_not_found() {
        let mock = MockService::new()
            // GET returns 404
            .on_get(
                "/api/v1/namespaces/new-ns",
                404,
                r#"{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"NotFound","code":404}"#,
            )
            // POST creates the namespace
            .on_post("/api/v1/namespaces", 201, &namespace_json("new-ns"));

        let client = mock.into_client();
        let result = ensure_namespace_exists(&client, "new-ns").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_namespace_error_propagates() {
        let mock = MockService::new()
            // Return a 500 error
            .on_get(
                "/api/v1/namespaces/error-ns",
                500,
                r#"{"kind":"Status","apiVersion":"v1","status":"Failure","reason":"InternalError","code":500}"#,
            );

        let client = mock.into_client();
        let result = ensure_namespace_exists(&client, "error-ns").await;

        assert!(result.is_err());
    }
}
