// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

/// Kubernetes annotation keys used by Outrider
pub mod annotations {
    /// When set to "true", enables secret syncing for this secret
    pub const ENABLED: &str = "outrider.geeko.me/enabled";
    /// Target namespace in downstream clusters (optional)
    pub const NAMESPACE: &str = "outrider.geeko.me/namespace";
}

/// The operator name used for server-side apply
pub const OPERATOR_NAME: &str = "outrider";

/// CRD polling configuration
pub mod crd {
    /// Initial polling interval in seconds when waiting for CRD
    pub const POLL_INTERVAL_SECS: u64 = 10;
    /// Maximum polling interval in seconds (exponential backoff cap)
    pub const POLL_MAX_INTERVAL_SECS: u64 = 60;
}
