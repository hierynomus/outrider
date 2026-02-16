// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0
use kube::{CustomResource, ResourceExt};
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Serialize, Deserialize, Clone, Debug, schemars::JsonSchema)]
#[kube(group = "provisioning.cattle.io", version = "v1", kind = "Cluster")]
#[kube(namespaced)]
#[kube(status = "ClusterStatus")]
#[serde(rename_all = "camelCase")]
pub struct ClusterSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

impl Cluster {
    /// Check if this cluster is ready based on its status conditions
    pub fn is_ready(&self) -> bool {
        self.status
            .as_ref()
            .and_then(|s| s.conditions.as_ref())
            .is_some_and(|conditions| {
                conditions
                    .iter()
                    .any(|c| c.condition_type == "Ready" && c.status == "True")
            })
    }

    /// Check if this is the local/management cluster
    pub fn is_local(&self) -> bool {
        self.name_any() == "local"
    }

    /// Get the name of the kubeconfig secret for this cluster
    pub fn kubeconfig_secret_name(&self) -> String {
        self.status
            .as_ref()
            .and_then(|s| s.client_secret_name.clone())
            .unwrap_or_else(|| format!("{}-kubeconfig", self.name_any()))
    }

    /// Get the internal cluster name from status
    pub fn internal_name(&self) -> String {
        self.status
            .as_ref()
            .map(|s| s.cluster_name.clone())
            .unwrap_or_else(|| self.name_any())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClusterStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret_name: Option<String>,
    pub cluster_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditions: Option<Vec<Condition>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    #[serde(rename = "type")]
    pub condition_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::api::ObjectMeta;

    fn make_cluster(name: &str, status: Option<ClusterStatus>) -> Cluster {
        Cluster {
            metadata: ObjectMeta {
                name: Some(name.to_string()),
                namespace: Some("fleet-default".to_string()),
                ..Default::default()
            },
            spec: ClusterSpec {
                kubernetes_version: None,
                local: None,
                display_name: None,
            },
            status,
        }
    }

    fn make_ready_condition() -> Condition {
        Condition {
            condition_type: "Ready".to_string(),
            status: "True".to_string(),
            message: None,
        }
    }

    fn make_not_ready_condition() -> Condition {
        Condition {
            condition_type: "Ready".to_string(),
            status: "False".to_string(),
            message: Some("Cluster is not ready".to_string()),
        }
    }

    #[test]
    fn test_is_ready_with_ready_condition() {
        let cluster = make_cluster(
            "test-cluster",
            Some(ClusterStatus {
                cluster_name: "c-12345".to_string(),
                client_secret_name: None,
                ready: Some(true),
                conditions: Some(vec![make_ready_condition()]),
            }),
        );

        assert!(cluster.is_ready());
    }

    #[test]
    fn test_is_ready_with_not_ready_condition() {
        let cluster = make_cluster(
            "test-cluster",
            Some(ClusterStatus {
                cluster_name: "c-12345".to_string(),
                client_secret_name: None,
                ready: Some(false),
                conditions: Some(vec![make_not_ready_condition()]),
            }),
        );

        assert!(!cluster.is_ready());
    }

    #[test]
    fn test_is_ready_with_no_conditions() {
        let cluster = make_cluster(
            "test-cluster",
            Some(ClusterStatus {
                cluster_name: "c-12345".to_string(),
                client_secret_name: None,
                ready: None,
                conditions: None,
            }),
        );

        assert!(!cluster.is_ready());
    }

    #[test]
    fn test_is_ready_with_no_status() {
        let cluster = make_cluster("test-cluster", None);
        assert!(!cluster.is_ready());
    }

    #[test]
    fn test_is_ready_with_multiple_conditions() {
        let cluster = make_cluster(
            "test-cluster",
            Some(ClusterStatus {
                cluster_name: "c-12345".to_string(),
                client_secret_name: None,
                ready: Some(true),
                conditions: Some(vec![
                    Condition {
                        condition_type: "Provisioned".to_string(),
                        status: "True".to_string(),
                        message: None,
                    },
                    make_ready_condition(),
                ]),
            }),
        );

        assert!(cluster.is_ready());
    }

    #[test]
    fn test_is_local_true() {
        let cluster = make_cluster("local", None);
        assert!(cluster.is_local());
    }

    #[test]
    fn test_is_local_false() {
        let cluster = make_cluster("downstream-cluster", None);
        assert!(!cluster.is_local());
    }

    #[test]
    fn test_kubeconfig_secret_name_from_status() {
        let cluster = make_cluster(
            "test-cluster",
            Some(ClusterStatus {
                cluster_name: "c-12345".to_string(),
                client_secret_name: Some("custom-kubeconfig-secret".to_string()),
                ready: None,
                conditions: None,
            }),
        );

        assert_eq!(cluster.kubeconfig_secret_name(), "custom-kubeconfig-secret");
    }

    #[test]
    fn test_kubeconfig_secret_name_fallback() {
        let cluster = make_cluster(
            "test-cluster",
            Some(ClusterStatus {
                cluster_name: "c-12345".to_string(),
                client_secret_name: None,
                ready: None,
                conditions: None,
            }),
        );

        assert_eq!(cluster.kubeconfig_secret_name(), "test-cluster-kubeconfig");
    }

    #[test]
    fn test_kubeconfig_secret_name_no_status() {
        let cluster = make_cluster("test-cluster", None);
        assert_eq!(cluster.kubeconfig_secret_name(), "test-cluster-kubeconfig");
    }

    #[test]
    fn test_internal_name_from_status() {
        let cluster = make_cluster(
            "test-cluster",
            Some(ClusterStatus {
                cluster_name: "c-12345".to_string(),
                client_secret_name: None,
                ready: None,
                conditions: None,
            }),
        );

        assert_eq!(cluster.internal_name(), "c-12345");
    }

    #[test]
    fn test_internal_name_fallback() {
        let cluster = make_cluster("test-cluster", None);
        assert_eq!(cluster.internal_name(), "test-cluster");
    }
}
