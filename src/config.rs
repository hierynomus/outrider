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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify environment variables
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn with_env_vars<F, R>(vars: &[(&str, Option<&str>)], f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Save original values and set new ones
        let originals: Vec<_> = vars
            .iter()
            .map(|(key, value)| {
                let original = env::var(key).ok();
                match value {
                    Some(v) => env::set_var(key, v),
                    None => env::remove_var(key),
                }
                (*key, original)
            })
            .collect();

        let result = f();

        // Restore original values
        for (key, original) in originals {
            match original {
                Some(v) => env::set_var(key, v),
                None => env::remove_var(key),
            }
        }

        result
    }

    #[test]
    fn test_from_env_success() {
        with_env_vars(
            &[
                ("DEFAULT_TARGET_NAMESPACE", Some("my-namespace")),
                ("TESTING_MODE", None),
            ],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.default_target_namespace, "my-namespace");
                assert!(!config.testing_mode);
            },
        );
    }

    #[test]
    fn test_from_env_with_testing_mode() {
        with_env_vars(
            &[
                ("DEFAULT_TARGET_NAMESPACE", Some("my-namespace")),
                ("TESTING_MODE", Some("true")),
            ],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.default_target_namespace, "my-namespace");
                assert!(config.testing_mode);
            },
        );
    }

    #[test]
    fn test_from_env_missing_namespace() {
        with_env_vars(
            &[
                ("DEFAULT_TARGET_NAMESPACE", None),
                ("TESTING_MODE", None),
            ],
            || {
                let result = Config::from_env();
                assert!(result.is_err());
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("DEFAULT_TARGET_NAMESPACE"));
            },
        );
    }

    #[test]
    fn test_from_env_invalid_testing_mode() {
        with_env_vars(
            &[
                ("DEFAULT_TARGET_NAMESPACE", Some("my-namespace")),
                ("TESTING_MODE", Some("not-a-bool")),
            ],
            || {
                let config = Config::from_env().unwrap();
                // Invalid bool parses to false
                assert!(!config.testing_mode);
            },
        );
    }
}