# StellPoker Infrastructure as Code

This directory contains Terraform modules for deploying StellPoker on AWS and Google Cloud Platform (GCP).

## Directory Structure

```
infrastructure/
├── terraform/
│   ├── main.tf              # Provider configuration and remote state
│   ├── variables.tf         # Input variables
│   ├── outputs.tf           # Output values
│   ├── aws_vpc.tf           # AWS VPC and networking
│   ├── aws_ecs.tf           # AWS ECS cluster and services
│   ├── aws_rds.tf           # AWS RDS database
│   ├── aws_cdn.tf           # AWS CloudFront CDN
│   └── gcp_gke.tf           # GCP GKE and Cloud SQL
├── DEPLOYMENT.md            # Detailed deployment guide
└── README.md                # This file
```

## What's Included

### AWS Infrastructure
- **VPC**: Multi-AZ VPC with public/private subnets
- **ECS**: Fargate cluster with auto-scaling
- **RDS**: PostgreSQL database with automated backups
- **CloudFront**: CDN with caching and logging
- **Load Balancer**: Application Load Balancer with health checks
- **Monitoring**: CloudWatch alarms and logs
- **Security**: KMS encryption, security groups, IAM roles

### GCP Infrastructure
- **GKE**: Kubernetes cluster with auto-scaling
- **Cloud SQL**: PostgreSQL database with automated backups
- **Cloud Load Balancer**: Global load balancing
- **Cloud CDN**: Content delivery network
- **Cloud Logging**: Centralized logging
- **Security**: Workload Identity, private VPC connectivity

## Quick Start

### Prerequisites
- Terraform >= 1.0
- AWS CLI configured (for AWS deployment)
- Google Cloud SDK (for GCP deployment)
- Docker (for building container images)

## Additional Guides

- [Self-hosted Soroban RPC node](../docs/soroban-rpc-node.md)

### Deploy to AWS

```bash
cd terraform

# Create terraform.tfvars
cat > terraform.tfvars << EOF
environment = "prod"
aws_region = "us-east-1"
coordinator_container_image = "YOUR_ECR_URI/coordinator:latest"
mpc_node_container_image = "YOUR_ECR_URI/mpc-node:latest"
db_password = "YOUR_SECURE_PASSWORD"
EOF

# Initialize, plan, and apply
terraform init
terraform plan
terraform apply
```

For a production-shaped staging deployment, start from
`terraform/environments/staging.aws.tfvars.example` and keep `environment = "staging"`.

### Deploy to GCP

```bash
cd terraform

# Create terraform.tfvars
cat > terraform.tfvars << EOF
gcp_project_id = "your-project-id"
gcp_region = "us-central1"
coordinator_container_image = "gcr.io/your-project-id/coordinator:latest"
mpc_node_container_image = "gcr.io/your-project-id/mpc-node:latest"
db_password = "YOUR_SECURE_PASSWORD"
EOF

# Initialize, plan, and apply
terraform init
terraform plan
terraform apply
```

## Key Variables

### Environment
- `environment`: Environment name (dev, staging, prod)
- `project_name`: Project name for resource naming

### AWS-Specific
- `aws_region`: AWS region (default: us-east-1)
- `vpc_cidr`: VPC CIDR block (default: 10.0.0.0/16)

### GCP-Specific
- `gcp_project_id`: GCP project ID
- `gcp_region`: GCP region (default: us-central1)
- `gke_node_count`: GKE node count (default: 1)

### Services
- `coordinator_container_image`: Coordinator Docker image URI
- `mpc_node_container_image`: MPC node Docker image URI

### Database
- `db_password`: Database master password (sensitive)
- `db_username`: Database username (default: coordinator)
- `db_allocated_storage`: Storage in GB (default: 100)

### Monitoring
- `enable_monitoring`: Enable CloudWatch/Cloud Logging
- `alarm_email`: Email for alarm notifications

For complete variable documentation, see `terraform/variables.tf`.

## Outputs

After applying Terraform, important outputs include:

```bash
# AWS Outputs
ALB DNS: terraform output alb_dns_name
RDS Endpoint: terraform output rds_endpoint
CDN Domain: terraform output cdn_domain_name

# GCP Outputs
GKE Cluster: terraform output gke_cluster_name
Cloud SQL: terraform output cloudsql_instance_name

# All outputs
terraform output -json
```

## Remote State

To enable remote state storage:

1. **AWS S3 + DynamoDB**:
   ```bash
   terraform init \
     -backend-config="bucket=your-state-bucket" \
     -backend-config="key=prod/terraform.tfstate" \
     -backend-config="dynamodb_table=terraform-locks"
   ```

2. **Enable in terraform.tfvars**:
   ```hcl
   enable_remote_state = true
   ```

## Deployment Workflow

1. **Plan Infrastructure**
   ```bash
   terraform plan -out=tfplan
   ```

2. **Review Changes**
   ```bash
   # Review the plan file
   cat tfplan
   ```

3. **Apply Configuration**
   ```bash
   terraform apply tfplan
   ```

4. **Verify Deployment**
   ```bash
   # Get endpoints
   terraform output deployment_summary
   
   # Test connectivity
   curl $(terraform output alb_dns_name)/api/health
   ```

## Scaling

### AWS ECS
```bash
terraform apply -var="coordinator_desired_count=5"
```

### GCP GKE
```bash
terraform apply -var="gke_node_count=5"
```

## Monitoring and Logging

### AWS CloudWatch
- View logs: `aws logs tail /ecs/stellpoker-cluster`
- CloudWatch Alarms for RDS CPU, storage, connections
- CloudFront metrics and cache stats

### GCP Cloud Logging
- View GKE logs via Cloud Logging
- Cloud SQL Query Insights
- Cloud Load Balancer metrics

## Cost Estimation

### AWS Pricing (Rough Estimates)
- **ECS Fargate**: ~$0.04/hour per GB-hour
- **RDS**: ~$0.50/hour for db.t3.medium
- **CloudFront**: ~$0.085/GB outbound
- **Data Transfer**: ~$0.02/GB outbound

### GCP Pricing (Rough Estimates)
- **GKE**: ~$0.25/cluster/hour + node costs
- **Compute Engine**: ~$0.05/hour per vCPU
- **Cloud SQL**: ~$0.50/hour for db-custom-2-7680
- **Cloud CDN**: ~$0.12/GB origin offload

See [AWS Calculator](https://calculator.aws/) and [GCP Calculator](https://cloud.google.com/products/calculator) for accurate estimates.

## Security Best Practices

1. **Secrets Management**
   - Database passwords in AWS Secrets Manager
   - Use `terraform-backend-config` for sensitive values
   - Never commit `terraform.tfvars` to version control

2. **Network Security**
   - Use private subnets for databases
   - Enable security groups with minimal permissions
   - Enable VPC Flow Logs for AWS

3. **Encryption**
   - RDS encryption at rest with KMS
   - HTTPS for all external communications
   - S3 bucket encryption for logs and backups

4. **Access Control**
   - Use IAM roles for service authentication
   - Workload Identity for GKE
   - Least privilege principle for policies

## Disaster Recovery

### AWS RDS Backups
- Automated backups: 30-day retention
- Manual snapshots for critical versions
- Cross-region replication (optional)

### GCP Cloud SQL Backups
- Automated daily backups
- Point-in-time recovery enabled
- Cross-region backup (optional)

## Troubleshooting

See [DEPLOYMENT.md](./DEPLOYMENT.md#troubleshooting) for detailed troubleshooting guides.

## Common Tasks

### Update Container Images
```bash
terraform apply -var="coordinator_container_image=NEW_IMAGE_URI"
```

### Enable Database Monitoring
```bash
terraform apply -var="enable_monitoring=true"
```

### Enable CDN
```bash
terraform apply -var="enable_cdn=true"
```

### Disable Resources (for cost reduction)
```bash
terraform apply -var="enable_alb=false" -var="enable_cdn=false"
```

## Maintenance

### Regular Updates
- Keep Terraform version updated
- Update provider versions: `terraform init -upgrade`
- Review provider changelogs for breaking changes

### Backup Terraform State
```bash
# Local backup
cp terraform.tfstate terraform.tfstate.backup

# AWS S3 backup
aws s3 cp terraform.tfstate s3://backups/terraform.tfstate
```

## Support and Resources

- [Terraform Documentation](https://www.terraform.io/docs)
- [AWS Provider Documentation](https://registry.terraform.io/providers/hashicorp/aws/latest/docs)
- [Google Provider Documentation](https://registry.terraform.io/providers/hashicorp/google/latest/docs)
- [Terraform Best Practices](https://developer.hashicorp.com/terraform/cloud-docs/recommended-practices)

## License

Same as the main StellPoker project.
