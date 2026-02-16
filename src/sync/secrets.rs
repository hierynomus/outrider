// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Secret listing, filtering, and copying utilities

use crate::config::Config;
use crate::constants::{annotations, OPERATOR_NAME};
use crate::error::Result;
use crate::kubernetes::{create_downstream_client, ensure_namespace_exists};
use crate::types::cluster::Cluster;
use k8s_openapi::api::core::v1::Secret;
use kube::{
    api::{ListParams, ObjectMeta, Patch, PatchParams},
    Api, Client, ResourceExt,
};
use tracing::{info, instrument};

/// Get all secrets that have the enabled annotation
#[instrument(skip(client))]
pub async fn get_enabled_secrets(client: &Client) -> Result<Vec<Secret>> {
    let secrets: Api<Secret> = Api::all(client.clone());
    let secret_list = secrets.list(&ListParams::default()).await?;

    Ok(secret_list
        .items
        .into_iter()
        .filter(is_secret_enabled)
        .collect())
}

/// Check if a secret has the enabled annotation set to "true"
pub fn is_secret_enabled(secret: &Secret) -> bool {
    secret
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(annotations::ENABLED))
        .is_some_and(|v| v == "true")
}

/// Get the target namespace for a secret from its annotation or use the default
pub fn get_target_namespace<'a>(secret: &'a Secret, config: &'a Config) -> &'a str {
    secret
        .metadata
        .annotations
        .as_ref()
        .and_then(|a| a.get(annotations::NAMESPACE))
        .map(|s| s.as_str())
        .unwrap_or(&config.default_target_namespace)
}

/// Copy a secret to a downstream cluster
#[instrument(
    skip(manager_client, secret, cluster, config),
    fields(
        secret = %format!("{}/{}", secret.namespace().unwrap_or_default(), secret.name_any()),
        cluster = %cluster.name_any()
    )
)]
pub async fn copy_secret_to_cluster(
    manager_client: &Client,
    secret: &Secret,
    cluster: &Cluster,
    config: &Config,
) -> Result<()> {
    let secret_name = secret.name_any();
    let source_namespace = secret.namespace().unwrap_or_default();
    let target_namespace = get_target_namespace(secret, config);

    info!(
        "Copying secret {}/{} to cluster {}",
        source_namespace,
        secret_name,
        cluster.name_any()
    );

    // Create client for downstream cluster
    let downstream_client = create_downstream_client(manager_client, cluster, config).await?;

    // Ensure target namespace exists in downstream cluster
    ensure_namespace_exists(&downstream_client, target_namespace).await?;

    // Copy the secret to downstream cluster
    let downstream_secrets: Api<Secret> = Api::namespaced(downstream_client, target_namespace);
    let new_secret = create_downstream_secret(secret, target_namespace);

    // Apply the secret (create or update)
    let pp = PatchParams::apply(OPERATOR_NAME).force();
    downstream_secrets
        .patch(&secret_name, &pp, &Patch::Apply(&new_secret))
        .await?;

    info!(
        "Successfully copied secret {}/{} to cluster {}/{}",
        source_namespace,
        secret_name,
        cluster.name_any(),
        target_namespace
    );

    Ok(())
}

/// Create a downstream secret by cloning and filtering outrider annotations
fn create_downstream_secret(secret: &Secret, target_namespace: &str) -> Secret {
    let filtered_annotations = secret.metadata.annotations.as_ref().map(|a| {
        a.iter()
            .filter(|(k, _)| !k.starts_with("outrider.geeko.me/"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    });

    Secret {
        metadata: ObjectMeta {
            name: secret.metadata.name.clone(),
            namespace: Some(target_namespace.to_string()),
            labels: secret.metadata.labels.clone(),
            annotations: filtered_annotations,
            ..Default::default()
        },
        data: secret.data.clone(),
        string_data: secret.string_data.clone(),
        type_: secret.type_.clone(),
        immutable: secret.immutable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::ByteString;
    use std::collections::BTreeMap;

    fn make_secret(
        name: &str,
        namespace: &str,
        annotations: Option<BTreeMap<String, String>>,
    ) -> Secret {
        Secret {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some(namespace.to_string()),
                annotations,
                ..Default::default()
            },
            data: Some(BTreeMap::from([(
                "password".to_string(),
                ByteString("secret123".as_bytes().to_vec()),
            )])),
            type_: Some("Opaque".to_string()),
            ..Default::default()
        }
    }

    fn make_config(default_namespace: &str) -> Config {
        Config {
            default_target_namespace: default_namespace.to_string(),
            testing_mode: false,
        }
    }

    #[test]
    fn test_is_secret_enabled_true() {
        let secret = make_secret(
            "my-secret",
            "default",
            Some(BTreeMap::from([(
                annotations::ENABLED.to_string(),
                "true".to_string(),
            )])),
        );

        assert!(is_secret_enabled(&secret));
    }

    #[test]
    fn test_is_secret_enabled_false_value() {
        let secret = make_secret(
            "my-secret",
            "default",
            Some(BTreeMap::from([(
                annotations::ENABLED.to_string(),
                "false".to_string(),
            )])),
        );

        assert!(!is_secret_enabled(&secret));
    }

    #[test]
    fn test_is_secret_enabled_no_annotation() {
        let secret = make_secret("my-secret", "default", None);
        assert!(!is_secret_enabled(&secret));
    }

    #[test]
    fn test_is_secret_enabled_wrong_annotation() {
        let secret = make_secret(
            "my-secret",
            "default",
            Some(BTreeMap::from([(
                "some.other/annotation".to_string(),
                "true".to_string(),
            )])),
        );

        assert!(!is_secret_enabled(&secret));
    }

    #[test]
    fn test_get_target_namespace_from_annotation() {
        let secret = make_secret(
            "my-secret",
            "default",
            Some(BTreeMap::from([(
                annotations::NAMESPACE.to_string(),
                "custom-namespace".to_string(),
            )])),
        );
        let config = make_config("default-ns");

        assert_eq!(get_target_namespace(&secret, &config), "custom-namespace");
    }

    #[test]
    fn test_get_target_namespace_fallback_to_config() {
        let secret = make_secret("my-secret", "default", None);
        let config = make_config("default-ns");

        assert_eq!(get_target_namespace(&secret, &config), "default-ns");
    }

    #[test]
    fn test_create_downstream_secret_filters_outrider_annotations() {
        let secret = make_secret(
            "my-secret",
            "source-ns",
            Some(BTreeMap::from([
                (annotations::ENABLED.to_string(), "true".to_string()),
                (annotations::NAMESPACE.to_string(), "target-ns".to_string()),
                ("keep.this/annotation".to_string(), "value".to_string()),
            ])),
        );

        let downstream = create_downstream_secret(&secret, "target-ns");

        let annotations = downstream.metadata.annotations.unwrap();
        assert!(!annotations.contains_key(annotations::ENABLED));
        assert!(!annotations.contains_key(annotations::NAMESPACE));
        assert_eq!(annotations.get("keep.this/annotation").unwrap(), "value");
    }

    #[test]
    fn test_create_downstream_secret_sets_target_namespace() {
        let secret = make_secret("my-secret", "source-ns", None);

        let downstream = create_downstream_secret(&secret, "target-ns");

        assert_eq!(downstream.metadata.namespace.unwrap(), "target-ns");
    }

    #[test]
    fn test_create_downstream_secret_preserves_data() {
        let secret = make_secret("my-secret", "source-ns", None);

        let downstream = create_downstream_secret(&secret, "target-ns");

        assert_eq!(downstream.data, secret.data);
        assert_eq!(downstream.type_, secret.type_);
    }

    #[test]
    fn test_create_downstream_secret_preserves_name() {
        let secret = make_secret("my-secret", "source-ns", None);

        let downstream = create_downstream_secret(&secret, "target-ns");

        assert_eq!(downstream.metadata.name.unwrap(), "my-secret");
    }
}
