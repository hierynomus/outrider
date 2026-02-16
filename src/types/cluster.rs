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
