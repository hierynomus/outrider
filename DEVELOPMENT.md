# Development Guide

## Prerequisites

- Rust 1.83 or later
- kubectl configured with access to a Rancher Manager cluster
- Access to downstream clusters managed by Rancher

## Local Development Setup

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Build the Project

```bash
cargo build
```

### 3. Run Tests (when implemented)

```bash
cargo test
```

### 4. Run Locally

Ensure your `~/.kube/config` points to the Rancher Manager cluster:

```bash
export DEFAULT_TARGET_NAMESPACE=fleet-default
export RUST_LOG=info
cargo run
```

## Testing the Operator

### 1. Create a Test Secret

```bash
kubectl create secret generic test-secret \
  --from-literal=username=admin \
  --from-literal=password=secret123 \
  -n fleet-default

kubectl annotate secret test-secret \
  outrider.geeko.me/enabled=true \
  -n fleet-default
```

### 2. Watch the Logs

The operator will log when it detects the secret and copies it to downstream clusters.

### 3. Verify on Downstream Cluster

```bash
# Get the kubeconfig for a downstream cluster
kubectl get secret <cluster-name>-kubeconfig -n cattle-system -o jsonpath='{.data.value}' | base64 -d > /tmp/downstream-kubeconfig

# Check if secret was copied
kubectl --kubeconfig=/tmp/downstream-kubeconfig get secret test-secret -n fleet-default
```

### 4. Test Custom Namespace

```bash
kubectl annotate secret test-secret \
  outrider.geeko.me/namespace=custom-ns \
  -n fleet-default --overwrite
```

The secret should now be copied to `custom-ns` in downstream clusters.

## Building Docker Image

```bash
docker build -t outrider:latest .
```

## Deployment to Kubernetes

### Manual Deployment

```bash
# Create namespace
kubectl create namespace outrider-system

# Create service account and RBAC
kubectl apply -f deploy/rbac.yaml

# Deploy the operator
kubectl apply -f deploy/deployment.yaml
```

### Using Helm (recommended)

```bash
helm install outrider ./charts/outrider \
  --namespace outrider-system \
  --create-namespace \
  --set config.defaultTargetNamespace=fleet-default
```

## Debugging

### Enable Debug Logging

```bash
export RUST_LOG=debug
cargo run
```

### Common Issues

1. **"Could not find Cluster API"**
   - Ensure you're running against a Rancher Manager cluster
   - Check that the Rancher CRDs are installed

2. **"Failed to get kubeconfig secret"**
   - Verify the cluster name is correct
   - Check that the kubeconfig secret exists in `cattle-system` namespace
   - Format should be: `{cluster-name}-kubeconfig`

3. **"Failed to create client from kubeconfig"**
   - The kubeconfig might be invalid
   - Check network connectivity to downstream cluster

## Code Structure

### Controllers

Both controllers follow the same pattern:
1. Watch for resources (Secrets or Clusters)
2. Filter by criteria (enabled annotation or Ready status)
3. Call reconciliation logic
4. Handle errors with retries

### Reconciliation Logic

- **SecretReconciler**: When a secret changes, copy to all ready clusters
- **ClusterReconciler**: When a cluster becomes ready, copy all enabled secrets

### Utilities

The `utils.rs` module contains shared logic:
- `get_enabled_secrets()` - Find all annotated secrets
- `get_ready_clusters()` - Find all ready clusters
- `copy_secret_to_cluster()` - Main copy logic
- `ensure_namespace_exists()` - Create namespace if needed

## Adding Features

### Example: Add Secret Filtering by Label

1. Update `get_enabled_secrets()` in `utils.rs`:

```rust
pub async fn get_enabled_secrets(client: &Client) -> Result<Vec<Secret>> {
    let secrets: Api<Secret> = Api::all(client.clone());
    let lp = ListParams::default();

    let secret_list = secrets.list(&lp).await?;

    Ok(secret_list
        .items
        .into_iter()
        .filter(|s| {
            // Check annotation
            let has_annotation = s.metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get(ENABLED_ANNOTATION))
                .map(|v| v == "true")
                .unwrap_or(false);

            // Check label (new feature)
            let has_label = s.metadata
                .labels
                .as_ref()
                .and_then(|l| l.get("app"))
                .map(|v| v == "outrider")
                .unwrap_or(true); // Default to true if no label

            has_annotation && has_label
        })
        .collect())
}
```

2. Update documentation
3. Add tests

## Performance Considerations

- The operator uses Kubernetes watch API for efficient event handling
- Reconciliation happens on-demand, not on polling
- Requeue intervals are configurable:
  - 5 minutes for normal requeues
  - 60 seconds for errors
  - 30 seconds for clusters not yet ready

## Security Notes

- The operator needs RBAC permissions to:
  - List/watch Secrets in all namespaces (manager cluster)
  - List/watch Cluster resources (manager cluster)
  - Get kubeconfig Secrets in cattle-system (manager cluster)
  - Create/patch Secrets in downstream clusters
  - Create Namespaces in downstream clusters

- Secrets are copied with their data unchanged
- Only metadata is sanitized (resourceVersion, uid, etc. removed)

## Next Steps

1. Implement comprehensive tests
2. Add Prometheus metrics
3. Add status conditions to track copy operations
4. Implement dry-run mode
5. Add webhook for validation