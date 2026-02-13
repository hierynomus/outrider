// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result};
use std::env;

/// Operator configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    /// Default namespace to copy secrets to in downstream clusters
    pub default_target_namespace: String,
    pub testing_mode: bool,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let default_target_namespace = env::var("DEFAULT_TARGET_NAMESPACE")
            .context("DEFAULT_TARGET_NAMESPACE environment variable not set")?;
         // For testing, uses the KUBECONFIG env var to create downstream clients instead of fetching kubeconfig from secrets
        let testing_mode: bool = env::var("TESTING_MODE").unwrap_or("false".to_string()).parse().unwrap_or(false);

        Ok(Config {
            default_target_namespace,
            testing_mode,
        })
    }
}