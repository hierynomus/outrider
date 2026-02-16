// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Kubernetes utilities for CRD discovery, client creation, and namespace management.

pub mod client;
pub mod crd;
pub mod namespaces;

pub use client::create_downstream_client;
pub use crd::wait_for_cluster_crd;
pub use namespaces::ensure_namespace_exists;
