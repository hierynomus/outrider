# Outrider Helm Chart

A Helm chart to deploy **Outrider**, a Kubernetes operator that propagates annotated `Secrets` to downstream clusters managed by Rancher.

## 🧪 Features

- Automatically propagates secrets with the annotation `outrider.geeko.me/enabled: true`
- Detects new downstream clusters and replicates secrets to them

---

## 🚀 Installation

```sh
helm repo add outrider https://hierynomus.github.io/charts
helm install outrider outrider/outrider
```

To install from a local directory:

```sh
helm install outrider ./charts/outrider
```

---

## 🔧 Configuration

| Key | Description | Default |
|-----|-------------|---------|
| `image.registry` | Container registry | `ghcr.io` |
| `image.repository` | Image repository | `hierynomus/outrider` |
| `image.tag` | Image tag | `latest` |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |
| `global.pullSecrets` | ImagePullSecrets to use | `[]` |
| `global.imageRegistry` | Overrides `.image.registry` globally | `""` |
| `fullnameOverride` | Overrides the full resource name | `""` |
| `resources.requests` / `limits` | CPU & memory settings | See `values.yaml` |

---

## 📦 Example

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: shared-secret
  annotations:
    outrider.geeko.me/enabled: "true"
data:
  key: value
```

This Secret will be automatically replicated to every downstream cluster.

---

## 🔐 RBAC

This chart creates the following Kubernetes resources:

- ServiceAccount
- ClusterRole with scoped permissions
- ClusterRoleBinding

---

## 🧹 Uninstall

```sh
helm uninstall outrider
```

---

## 👷 Maintainers

- @hierynomus
