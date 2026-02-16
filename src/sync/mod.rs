// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Secret and cluster synchronization logic.

pub mod manager;
pub mod secrets;

pub use manager::{SyncEvent, SyncManager, SyncManagerHandle};
pub use secrets::{copy_secret_to_cluster, get_enabled_secrets};
