# DSB Helm Chart

This Helm chart deploys the DSB (Distributed Sandboxes) control plane on Kubernetes.

## Prerequisites

- Kubernetes 1.25+
- Helm 3.8+
- PostgreSQL 15+ (can be deployed separately or as a subchart)

### Kubernetes sandbox backend (`config.sandbox.backend: kubernetes`)

The `dsb` binary must be compiled with the `kubernetes` Cargo feature (the default image build only enables the Docker backend). Build the server image with:

```bash
FEATURES=kubernetes docker compose -f docker/docker-compose.yml build dsb-server
```

Or set the same `FEATURES` build arg in CI. Without this, the process exits with “Kubernetes feature not enabled” when the Kubernetes backend is selected.

## Installing the Chart

To install the chart with the release name `dsb`:

```bash
helm install dsb ./deployment/helm/dsb \
  --set postgres.password=your-postgres-password \
  --set secrets.apiKey=your-api-key
```

## Uninstalling the Chart

To uninstall/delete the `dsb` deployment:

```bash
helm uninstall dsb
```

## Configuration

The following table lists the configurable parameters of the DSB chart and their default values.

### Global Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of DSB server replicas | `1` |
| `image.repository` | DSB server image repository | `dsb/server` |
| `image.tag` | DSB server image tag | `latest` |
| `image.pullPolicy` | Image pull policy | `IfNotPresent` |

### Service Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `service.type` | Kubernetes service type | `ClusterIP` |
| `service.port` | Service port | `8080` |

### Ingress Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `ingress.enabled` | Enable ingress | `false` |
| `ingress.className` | Ingress class name | `""` |
| `ingress.annotations` | Ingress annotations | `{}` |
| `ingress.hosts` | Ingress hosts | `[{host: dsb.local, paths: [{path: /, pathType: Prefix}]}]` |
| `ingress.tls` | Ingress TLS configuration | `[]` |

### Database Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `postgres.host` | PostgreSQL host | `postgres` |
| `postgres.port` | PostgreSQL port | `5432` |
| `postgres.database` | PostgreSQL database name | `dsb` |
| `postgres.user` | PostgreSQL user | `postgres` |
| `postgres.password` | PostgreSQL password | `""` |
| `postgres.existingSecret` | Existing secret for PostgreSQL password | `""` |
| `postgres.existingSecretKey` | Key in existing secret | `password` |

### API Key Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `secrets.apiKey` | API key for DSB | auto-generated |
| `secrets.adminApiKey` | Admin API key | auto-generated |
| `secrets.webTerminalApiKey` | Web Terminal API key | auto-generated |
| `secrets.sshGatewayApiKey` | SSH Gateway API key | auto-generated |
| `secrets.vncApiKey` | VNC API key | auto-generated |
| `secrets.existingSecret` | Use existing secret for all API keys | `""` |

### DSB Configuration Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `config.server.host` | Server bind address | `0.0.0.0` |
| `config.server.port` | Server port | `8080` |
| `config.server.requireAuth` | Require authentication | `true` |
| `config.docker.registry` | Docker registry | `docker.io` |
| `config.docker.defaultImage` | Default sandbox image (full `dsb/sandbox` for daemon + browser tools) | `dsb/sandbox:latest` |
| `config.sandbox.defaultInactivityTimeout` | Default inactivity timeout (minutes) | `30` |
| `config.sandbox.cleanupDryRun` | Cleanup dry run mode | `false` |
| `config.logging.level` | Log level | `info` |
| `config.logging.format` | Log format | `json` |
| `kubernetes.egressProxy.configMapName` | Optional ConfigMap (release namespace) with `HTTP_PROXY`, `HTTPS_PROXY`, `NO_PROXY` for corporate egress; forwarded to DSB and into every sandbox Pod | `""` |

### Egress proxy (Kubernetes / corporate clusters)

If sandboxes need **Internet access** through an egress proxy (common in enterprise Kubernetes clusters), create a ConfigMap in the **same namespace as the DSB release** with the proxy endpoints, then point Helm at it.

1. **Create the ConfigMap** (adjust `NO_PROXY` for your VPC CIDRs and internal DNS suffixes; your platform docs may provide a standard list):

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: dsb-egress-proxy
  namespace: dsb   # same as Helm release namespace
data:
  HTTPS_PROXY: "http://proxy.example.com:3128"
  HTTP_PROXY: "http://proxy.example.com:3128"
  NO_PROXY: "localhost,127.0.0.1,.svc,.cluster.local"
```

2. **Enable it in Helm** (values or `--set`):

```yaml
kubernetes:
  egressProxy:
    configMapName: dsb-egress-proxy
```

DSB reads `HTTP_PROXY` / `HTTPS_PROXY` / `NO_PROXY` at startup and merges them into each sandbox Pod’s environment (same behavior as the Docker backend). File-based `docker.proxy_env` from config is merged with those process variables; **process env wins** on duplicate keys (so ConfigMap-injected proxy overrides static YAML).

The sandbox image launches Chromium via `chromium-launch.sh`, which maps `HTTPS_PROXY` / `HTTP_PROXY` to **`--proxy-server`** and `NO_PROXY` to **`--proxy-bypass-list`** (Chromium often ignores `HTTP_PROXY` alone for real navigations under CDP + crawl4ai). DSB’s Kubernetes backend also **extends** `NO_PROXY` / `no_proxy` on sandbox Pods when a proxy is set but the list does not already include `.svc.cluster.local`, so loopback/CDP and in-cluster DNS are not sent through the corporate proxy.

You can instead set the same variables on the DSB Deployment using `extraEnv` with `configMapKeyRef`; avoid duplicating both unless you know you need overrides.

### Persistence Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `persistence.enabled` | Enable persistent storage for static files | `true` |
| `persistence.storageClass` | Storage class | `""` |
| `persistence.size` | PVC size | `10Gi` |

### Resource Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `resources.limits.cpu` | CPU limit | `1000m` |
| `resources.limits.memory` | Memory limit | `1Gi` |
| `resources.requests.cpu` | CPU request | `500m` |
| `resources.requests.memory` | Memory request | `512Mi` |

## Example Values

### Minimal Configuration

```yaml
postgres:
  host: "my-postgres.db.svc.cluster.local"
  password: "secure-password"

secrets:
  apiKey: "my-api-key"
  adminApiKey: "my-admin-api-key"
```

### Production Configuration

```yaml
replicaCount: 3

image:
  tag: "1.0.0"

ingress:
  enabled: true
  className: "nginx"
  hosts:
    - host: dsb.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: dsb-tls
      hosts:
        - dsb.example.com

postgres:
  host: "postgres.postgres.svc.cluster.local"
  existingSecret: "postgres-credentials"

secrets:
  existingSecret: "dsb-api-keys"

resources:
  limits:
    cpu: 2000m
    memory: 2Gi
  requests:
    cpu: 1000m
    memory: 1Gi

autoscaling:
  enabled: true
  minReplicas: 2
  maxReplicas: 10
```

## Using Existing Secrets

For production deployments, it's recommended to create secrets separately:

```bash
# Create API keys secret
kubectl create secret generic dsb-api-keys \
  --from-literal=api-key=$(openssl rand -hex 32) \
  --from-literal=admin-api-key=$(openssl rand -hex 32) \
  --from-literal=web-terminal-api-key=$(openssl rand -hex 32) \
  --from-literal=ssh-gateway-api-key=$(openssl rand -hex 32) \
  --from-literal=vnc-api-key=$(openssl rand -hex 32)

# Create database secret
kubectl create secret generic postgres-credentials \
  --from-literal=password=your-postgres-password
```

Then reference them in values:

```yaml
secrets:
  existingSecret: "dsb-api-keys"

postgres:
  existingSecret: "postgres-credentials"
```

## Health Checks

The chart includes liveness and readiness probes:

- **Liveness**: `/health` endpoint (initial delay: 60s, period: 30s)
- **Readiness**: `/health` endpoint (initial delay: 30s, period: 10s)
