
# Outrider 🤠

![Outrider banner](https://raw.githubusercontent.com/hierynomus/outrider/main/assets/outrider-banner.png)

Like the outriders in the Wild West, this operator rides ahead to ensure your secrets are safely delivered to all your downstream clusters before your GitOps deployments kick off. No more manually creating your secrets on your downstream clusters or waiting around for secrets to sync or worrying about missing credentials in your Fleet-managed clusters. Outrider has got you covered, blazing a trail of secure secret distribution across your Rancher and Kubernetes landscape.

## Overview

Outrider watches for:

1. **Secrets** in the Rancher Manager cluster with the annotation `outrider.geeko.me/enabled: "true"`
2. **Rancher Clusters** (provisioning.cattle.io/v1) that reach Ready status

When either event occurs, Outrider copies all annotated secrets to any ready downstream clusters.

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

Use the Helm chart for production deployments.

## License

Apache 2.0

Built with 💚 in Rust, powered by kube-rs.
