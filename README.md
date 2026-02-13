# Outrider 🤠

A Kubernetes operator for Rancher that automatically copies secrets from the Rancher Manager cluster to downstream clusters before Fleet GitOps deployments.

## Overview

Outrider watches for:

1. **Secrets** in the Rancher Manager cluster with the annotation `outrider.geeko.me/enabled: "true"`
2. **Rancher Clusters** (provisioning.cattle.io/v1) that reach Ready status

When either event occurs, Outrider copies all annotated secrets to all ready downstream clusters.

## Features

- **Automatic Secret Distribution**: Copies secrets to downstream clusters automatically
- **Namespace Control**: Configure target namespace per secret or use a default
- **Real-time Updates**: Watches for secret updates and re-syncs automatically
- **Cluster-aware**: Only copies to ready clusters
- **Idempotent**: Safe to run continuously, re-copying is handled gracefully

## Annotations

### On Secrets (in Manager Cluster)

- `outrider.geeko.me/enabled: "true"` - **Required**. Marks the secret for copying
- `outrider.geeko.me/namespace: "target-ns"` - **Optional**. Override target namespace (defaults to configured default)

### Example

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: my-secret
  namespace: fleet-default
  annotations:
    outrider.geeko.me/enabled: "true"
    outrider.geeko.me/namespace: "custom-namespace"
type: Opaque
data:
  username: YWRtaW4=
  password: MWYyZDFlMmU2N2Rm
```

## Configuration

The operator is configured via environment variables:

- `DEFAULT_TARGET_NAMESPACE` - **Required**. Default namespace to copy secrets to in downstream clusters

## Architecture

### Controllers

1. **SecretReconciler**: Watches annotated Secrets in the manager cluster
   - Triggers on secret create/update
   - Copies to all ready downstream clusters

2. **ClusterReconciler**: Watches Rancher Cluster resources
   - Triggers when cluster becomes Ready
   - Copies all annotated secrets to the new cluster

### Workflow

```
┌─────────────────┐
│  Manager Cluster│
│                 │
│  ┌───────────┐  │     ┌──────────────────┐
│  │  Secret   │  │────▶│   Outrider       │
│  │ (annotated)│  │     │                  │
│  └───────────┘  │     │  - SecretCtrl    │
│                 │     │  - ClusterCtrl   │
│  ┌───────────┐  │     └────────┬─────────┘
│  │ Cluster   │  │              │
│  │ (Ready)   │  │              │
│  └───────────┘  │              │
└─────────────────┘              │
                                 │ Copies secrets
                                 ▼
                    ┌─────────────────────────┐
                    │  Downstream Clusters    │
                    │                         │
                    │  ┌─────────────────┐    │
                    │  │  target-ns      │    │
                    │  │  ┌───────────┐  │    │
                    │  │  │  Secret   │  │    │
                    │  │  └───────────┘  │    │
                    │  └─────────────────┘    │
                    └─────────────────────────┘
```

## Building

```bash
cargo build --release
```

The binary will be at `target/release/outrider`.

## Running Locally

```bash
export DEFAULT_TARGET_NAMESPACE=fleet-default
cargo run
```

## Deployment

Use the Helm chart (to be created) for production deployments.

## Development

### Project Structure

```
outrider/
├── src/
│   ├── main.rs              # Entry point, controller setup
│   ├── config.rs            # Configuration management
│   ├── error.rs             # Error types
│   └── controllers/
│       ├── mod.rs           # Controller module
│       ├── secret.rs        # SecretReconciler
│       ├── cluster.rs       # ClusterReconciler
│       └── utils.rs         # Shared utilities
├── Cargo.toml               # Dependencies
└── README.md                # This file
```

### Key Dependencies

- `kube` - Kubernetes client and runtime
- `k8s-openapi` - Kubernetes API types
- `tokio` - Async runtime
- `tracing` - Logging

## License

TBD