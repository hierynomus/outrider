// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OutriderError {
    #[error("Kubernetes API error: {0}")]
    KubeError(#[from] kube::Error),

    #[error("Failed to parse kubeconfig: {0}")]
    KubeconfigError(String),

    #[error("Cluster not ready: {0}")]
    ClusterNotReady(String),

    #[error("Secret copy failed: {0}")]
    SecretCopyError(String),

    #[error("Namespace creation failed: {0}")]
    NamespaceError(String),

    #[error("Invalid annotation: {0}")]
    InvalidAnnotation(String),
}

pub type Result<T> = std::result::Result<T, OutriderError>;