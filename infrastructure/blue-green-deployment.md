# Blue-Green Deployment Strategy for Coordinator

This document defines a blue-green deployment strategy for the MPC coordinator service, enabling zero-downtime releases by running two identical environments (blue and green) and switching traffic atomically after health checks confirm the new instance is ready.

---

## 1. Architecture Overview

```
                 ┌─────────────────────────┐
                 │     Load Balancer        │
                 │  (ALB / K8s Ingress)     │
                 └──────┬──────────┬───────┘
                        │          │
                   weight=100   weight=0
                        │          │
                 ┌──────▼──┐  ┌───▼──────┐
                 │  Green   │  │   Blue   │
                 │ (active) │  │(standby) │
                 │ coord.   │  │ coord.   │
                 └──────┬───┘  └───┬──────┘
                        │          │
                 ┌──────▼──────────▼───────┐
                 │     Upstream Services    │
                 │  (MPC Nodes, Soroban    │
                 │   RPC, PostgreSQL)       │
                 └─────────────────────────┘
```

- **Green** — the currently active production instance. Receives 100% of traffic.
- **Blue** — the new version deployed alongside Green. Receives traffic only after health checks pass.
- **Load balancer** — routes traffic between the two instances based on target group weights.
- **Upstream services** — MPC nodes, Soroban RPC, and PostgreSQL are shared; both instances connect to the same upstreams.

---

## 2. Session State Considerations

The coordinator manages session state primarily in memory with optional PostgreSQL persistence.

### In-Memory Sessions (MpcSession, TableSession)

Active sessions exist only in the running coordinator's memory. During a blue-green cutover:

- **In-flight MPC proofs** (deal, reveal, showdown) will fail on the old instance.
- **Table sessions** are rehydrated from the Soroban chain via `ensure_session_exists()`.
- **Archived sessions** (Issue #259) are persisted to the file system or S3 and accessible from either instance.

### Session Migration

The coordinator supports session migration (Issue #264) to move in-progress sessions between instances:

```bash
# List active sessions on the old instance
curl -H "Authorization: Bearer <token>" \
  https://green-coordinator/api/admin/sessions

# Migrate sessions to the new instance
curl -X POST -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "session_id": "<session-id>",
    "table_id": <table-id>,
    "to_instance_id": "<blue-instance-id>"
  }' \
  https://green-coordinator/api/admin/migrations/initiate
```

### Graceful Connection Draining

The coordinator's `/api/health` endpoint returns `200 OK` while the instance is healthy and ready to serve traffic. Before decommissioning the old instance:

1. Set the load balancer drain timeout to at least 30 seconds.
2. Allow in-flight requests to complete before target deregistration.
3. New MPC proof requests will be routed to the new instance by the load balancer.

---

## 3. Kubernetes Implementation

### 3.1 Service-Based Routing

Kubernetes uses separate `Service` resources for blue and green, with a single `Ingress` controlling traffic weight.

```yaml
# coordinator-green.yaml
apiVersion: v1
kind: Service
metadata:
  name: coordinator-green
  labels:
    app: coordinator
    color: green
spec:
  selector:
    app: coordinator
    color: green
  ports:
    - port: 8080
      targetPort: 8080
---
# coordinator-blue.yaml
apiVersion: v1
kind: Service
metadata:
  name: coordinator-blue
  labels:
    app: coordinator
    color: blue
spec:
  selector:
    app: coordinator
    color: blue
  ports:
    - port: 8080
      targetPort: 8080
```

### 3.2 Ingress Traffic Splitting

Use an ingress controller with weight-based routing (e.g., nginx-ingress or Contour):

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: coordinator-ingress
  annotations:
    nginx.ingress.kubernetes.io/canary: "true"
    nginx.ingress.kubernetes.io/canary-weight: "0"
spec:
  rules:
    - host: coordinator.stellpoker.example
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: coordinator-green
                port:
                  number: 8080
```

During deployment, increase the canary weight to shift traffic:

```bash
# Deploy Blue and route 0% initially
kubectl apply -f coordinator-blue.yaml

# Wait for Blue health checks (see section 4)
kubectl rollout status deployment/coordinator-blue

# Route 100% to Blue
kubectl annotate ingress/coordinator-ingress \
  nginx.ingress.kubernetes.io/canary-weight="100"
# Or update the main backend to point to Blue
kubectl patch ingress/coordinator-ingress \
  --type='json' \
  -p='[{"op": "replace", "path": "/spec/rules/0/http/paths/0/backend/service/name", "value": "coordinator-blue"}]'

# Scale down Green after cutover
kubectl scale deployment/coordinator-green --replicas=0
```

### 3.3 Helm Values for Blue-Green

```yaml
# values-blue-green.yaml
coordinator:
  replicaCount: 2
  deploymentStrategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0

ingress:
  enabled: true
  annotations:
    nginx.ingress.kubernetes.io/canary: "true"

service:
  type: ClusterIP
  port: 8080

livenessProbe:
  httpGet:
    path: /api/health
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 10
  timeoutSeconds: 5
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /api/health
    port: 8080
  initialDelaySeconds: 5
  periodSeconds: 5
  timeoutSeconds: 3
  successThreshold: 1
  failureThreshold: 2
```

---

## 4. AWS ECS Implementation

### 4.1 ECS CodeDeploy Blue-Green

AWS ECS supports blue-green deployments natively through CodeDeploy. The Terraform configuration can be extended:

```hcl
# aws_codedeploy_app
resource "aws_codedeploy_app" "coordinator" {
  compute_platform = "ECS"
  name             = "${var.project_name}-coordinator"
}

# aws_codedeploy_deployment_group with blue-green config
resource "aws_codedeploy_deployment_group" "coordinator" {
  app_name              = aws_codedeploy_app.coordinator.name
  deployment_group_name = "${var.project_name}-coordinator-bg"
  service_role_arn      = aws_iam_role.codedeploy.arn

  deployment_config_name = "CodeDeployDefault.ECSAllAtOnce"

  blue_green_deployment_config {
    deployment_ready_option {
      action_on_timeout = "CONTINUE_DEPLOYMENT"
    }

    terminate_blue_instances_on_deployment_success {
      action                           = "TERMINATE"
      termination_wait_time_in_minutes = 5
    }
  }

  deployment_style {
    deployment_option = "WITH_TRAFFIC_CONTROL"
    deployment_type   = "BLUE_GREEN"
  }

  ecs_service {
    cluster_name = aws_ecs_cluster.main.name
    service_name = aws_ecs_service.coordinator.name
  }

  load_balancer_info {
    target_group_pair_info {
      prod_traffic_route {
        listener_arns = [aws_lb_listener.coordinator.arn]
      }

      target_group {
        name = aws_lb_target_group.coordinator_blue.name
      }
      target_group {
        name = aws_lb_target_group.coordinator_green.name
      }
    }
  }
}
```

### 4.2 ALB Configuration

Two target groups are required, one for each color:

```hcl
resource "aws_lb_target_group" "coordinator_blue" {
  name        = "${var.project_name}-co-b"
  port        = var.coordinator_container_port
  protocol    = "HTTP"
  vpc_id      = data.aws_vpc.main.id
  target_type = "ip"

  health_check {
    path     = "/api/health"
    interval = 30
    timeout  = 5
    matcher  = "200-299"
  }

  tags = { Color = "blue" }
}

resource "aws_lb_target_group" "coordinator_green" {
  name        = "${var.project_name}-co-g"
  port        = var.coordinator_container_port
  protocol    = "HTTP"
  vpc_id      = data.aws_vpc.main.id
  target_type = "ip"

  health_check {
    path     = "/api/health"
    interval = 30
    timeout  = 5
    matcher  = "200-299"
  }

  tags = { Color = "green" }
}
```

---

## 5. Deployment Workflow

### 5.1 Step-by-Step Blue-Green Cutover

```
┌─────────────────────────────────────────────────────────┐
│                    1. Pre-Deployment                     │
│  - Green is active, receiving 100% traffic               │
│  - Verify Green health: curl /api/health                 │
│  - Notify team of upcoming deployment                    │
├─────────────────────────────────────────────────────────┤
│                   2. Deploy Blue                          │
│  - Push new Docker image                                 │
│  - Start Blue instance (same upstream config as Green)   │
│  - Blue connects to same DB, MPC nodes, Soroban RPC      │
├─────────────────────────────────────────────────────────┤
│                 3. Health Check Gate                      │
│  - Verify Blue /api/health returns 200                   │
│  - Check MPC node connectivity in health response        │
│  - Validate Soroban RPC connectivity                     │
│  - Optionally run smoke tests against Blue endpoint      │
│  - ALL checks must pass before proceeding                │
├─────────────────────────────────────────────────────────┤
│                  4. Traffic Switch                        │
│  - Route 100% traffic from Green → Blue                  │
│  - In K8s: update ingress backend or canary weight       │
│  - In ECS: CodeDeploy shifts traffic automatically       │
├─────────────────────────────────────────────────────────┤
│                  5. Post-Deployment Verification          │
│  - Verify Green health endpoint returns 200              │
│     (Green is still running but receiving 0 traffic)     │
│  - Run integration tests against production endpoint     │
│  - Monitor error rates and latency for 5 minutes         │
├─────────────────────────────────────────────────────────┤
│                  6. Scale Down Green                      │
│  - If verification passes, terminate Green instances     │
│  - Keep Green's target group for next deployment cycle   │
│  - Log the deployment in the audit trail                 │
└─────────────────────────────────────────────────────────┘
```

### 5.2 Rollback Procedure

If the Blue deployment exhibits issues:

```bash
# K8s: Route traffic back to Green
kubectl patch ingress/coordinator-ingress \
  --type='json' \
  -p='[{"op": "replace", "path": "/spec/rules/0/http/paths/0/backend/service/name", "value": "coordinator-green"}]'

# Scale Blue down
kubectl scale deployment/coordinator-blue --replicas=0

# ECS: Trigger rollback in CodeDeploy
aws deploy stop-deployment \
  --deployment-id <deployment-id> \
  --auto-rollback-enabled

# Verify Green health
curl https://coordinator.stellpoker.example/api/health
```

### 5.3 Smoke Tests

Run these checks against the new Blue instance before traffic switch:

```bash
# 1. Health endpoint
curl -s http://blue-coordinator:8080/api/health | jq '.mpc_nodes | all(.connected)'

# 2. List open tables
curl -s http://blue-coordinator:8080/api/tables/open | jq '.tables | length'

# 3. Metrics endpoint
curl -s http://blue-coordinator:8080/metrics | grep coordinator_requests_total

# 4. Database connectivity (if DATABASE_URL configured)
curl -s http://blue-coordinator:8080/api/admin/sessions | jq '.count'
```

---

## 6. Monitoring and Alerts

### Key Metrics During Blue-Green

| Metric | Expected | Alert Threshold |
|---|---|---|
| Blue health check success | 100% | < 100% after 30s |
| Request error rate (5xx) | < 1% | > 5% for 2 minutes |
| p99 latency | < 500ms | > 2s for 1 minute |
| Active MPC sessions | Non-decreasing | Drop > 50% in 1 minute |
| MPC node connectivity | All connected | Any disconnected |

### Grafana Dashboard Queries

```promql
# Error rate comparison between blue and green
sum(rate(coordinator_request_errors_total{route!~".*health.*"}[5m])) by (job)

# Active sessions
coordinator_active_mpc_sessions

# Latency p99
histogram_quantile(0.99, rate(coordinator_request_latency_seconds_bucket[5m]))
```

---

## 7. Environment Configuration

### Kubernetes

Deploy two identical Helm releases:

```bash
# Green (active)
helm upgrade --install coordinator-green ./infrastructure/helm/coordinator \
  --namespace stellpoker \
  -f values-blue-green.yaml \
  --set 'color=green' \
  --set 'replicaCount=2'

# Blue (standby)
helm upgrade --install coordinator-blue ./infrastructure/helm/coordinator \
  --namespace stellpoker \
  -f values-blue-green.yaml \
  --set 'color=blue' \
  --set 'replicaCount=2' \
  --set 'image.tag=sha-abc1234'
```

### ECS

Use CodeDeploy tags to distinguish revisions:

```bash
aws ecs update-service \
  --cluster stellpoker-prod \
  --service coordinator \
  --task-definition coordinator:42 \
  --deployment-controller type=CODE_DEPLOY
```

---

## 8. Rollback Readiness Checklist

- [ ] Green instance is still running and healthy
- [ ] Green target group has healthy instances registered
- [ ] Database migration (if any) is backward-compatible
- [ ] No irreversible data mutations were applied by Blue
- [ ] Rollback script has been tested in staging
- [ ] Monitoring dashboards are loaded and visible
- [ ] On-call engineer is notified of the deployment window

---

## 9. Comparison: K8s vs ECS Blue-Green

| Aspect | Kubernetes | AWS ECS + CodeDeploy |
|---|---|---|
| Traffic splitting | Ingress canary annotations | Native ALB target group swap |
| Deployment control | Manual `kubectl` commands | Automated CodeDeploy pipeline |
| Health check gate | Readiness probe + custom script | CodeDeploy lifecycle hooks |
| Rollback | Revert ingress annotation | `aws deploy stop-deployment` |
| Session state | In-memory (lost on restart) | In-memory (lost on restart) |
| Setup complexity | Requires ingress controller | Native ECS feature |
| Audit trail | K8s events + application logs | CodeDeploy deployment history |

Both approaches provide the same core guarantee: traffic is only sent to the new instance after it passes health checks, and rollback is a single action.
