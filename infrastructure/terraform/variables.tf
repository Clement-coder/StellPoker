variable "environment" {
  description = "Environment name (dev, staging, prod)"
  type        = string
  default     = "prod"

  validation {
    condition     = contains(["dev", "staging", "prod"], var.environment)
    error_message = "Environment must be one of: dev, staging, prod"
  }
}

variable "project_name" {
  description = "Project name"
  type        = string
  default     = "stellpoker"
}

# AWS Configuration
variable "aws_region" {
  description = "AWS region"
  type        = string
  default     = "us-east-1"
}

variable "aws_availability_zones" {
  description = "AWS availability zones"
  type        = list(string)
  default     = ["us-east-1a", "us-east-1b", "us-east-1c"]
}

variable "enable_remote_state" {
  description = "Enable remote Terraform state storage"
  type        = bool
  default     = false
}

# VPC Configuration
variable "vpc_cidr" {
  description = "VPC CIDR block"
  type        = string
  default     = "10.0.0.0/16"
}

variable "enable_nat_gateway" {
  description = "Enable NAT Gateway for private subnets"
  type        = bool
  default     = true
}

variable "enable_vpn_gateway" {
  description = "Enable VPN Gateway"
  type        = bool
  default     = false
}

# ECS Configuration
variable "ecs_cluster_name" {
  description = "ECS cluster name"
  type        = string
  default     = "stellpoker-cluster"
}

variable "coordinator_container_image" {
  description = "Coordinator Docker image URI"
  type        = string
}

variable "coordinator_container_port" {
  description = "Coordinator container port"
  type        = number
  default     = 8080
}

variable "coordinator_cpu" {
  description = "Coordinator task CPU units"
  type        = number
  default     = 512
}

variable "coordinator_memory" {
  description = "Coordinator task memory in MB"
  type        = number
  default     = 1024
}

variable "coordinator_desired_count" {
  description = "Desired number of coordinator tasks"
  type        = number
  default     = 2
}

variable "mpc_node_container_image" {
  description = "MPC Node Docker image URI"
  type        = string
}

variable "mpc_node_container_port" {
  description = "MPC Node HTTP port"
  type        = number
  default     = 8101
}

variable "mpc_node_cpu" {
  description = "MPC Node task CPU units"
  type        = number
  default     = 1024
}

variable "mpc_node_memory" {
  description = "MPC Node task memory in MB"
  type        = number
  default     = 2048
}

variable "mpc_node_count" {
  description = "Number of MPC nodes"
  type        = number
  default     = 3
}

# Database Configuration
variable "db_engine" {
  description = "Database engine (postgres, cloudsql-postgres)"
  type        = string
  default     = "postgres"
}

variable "db_version" {
  description = "Database version"
  type        = string
  default     = "15.2"
}

variable "db_instance_class" {
  description = "Database instance class"
  type        = string
  default     = "db.t3.medium"
}

variable "db_allocated_storage" {
  description = "Allocated storage in GB"
  type        = number
  default     = 100
}

variable "db_backup_retention_period" {
  description = "Database backup retention in days"
  type        = number
  default     = 30
}

variable "db_username" {
  description = "Database master username"
  type        = string
  default     = "coordinator"
  sensitive   = true
}

variable "db_password" {
  description = "Database master password"
  type        = string
  sensitive   = true
}

# RDS Configuration (AWS)
variable "rds_multi_az" {
  description = "Enable Multi-AZ for RDS"
  type        = bool
  default     = true
}

variable "rds_storage_encrypted" {
  description = "Enable RDS encryption at rest"
  type        = bool
  default     = true
}

variable "rds_backup_window" {
  description = "RDS backup window (UTC)"
  type        = string
  default     = "03:00-04:00"
}

variable "rds_maintenance_window" {
  description = "RDS maintenance window"
  type        = string
  default     = "sun:04:00-sun:05:00"
}

# Load Balancer Configuration
variable "enable_alb" {
  description = "Enable Application Load Balancer"
  type        = bool
  default     = true
}

# TLS termination for public API gateway (ALB)
variable "acm_certificate_arn" {
  description = "ACM certificate ARN for ALB HTTPS listener"
  type        = string
  default     = ""
}

# WAF (rate limiting, IP allowlisting)
variable "enable_waf" {
  description = "Enable AWS WAFv2 Web ACL for coordinator"
  type        = bool
  default     = true
}

variable "waf_allowed_ips" {
  description = "Allowed IPv4 CIDRs for public API access (WAF IP set). Empty => deny all."
  type        = list(string)
  default     = []
}

# WAF rate limit threshold (requests per 5-minute interval as per AWS rate-based statement)
variable "waf_rate_limit_requests" {
  description = "WAF rate limit per IP (requests per 5 minutes)"
  type        = number
  default     = 300
}

# AWS Shield (DDoS)
variable "enable_shield" {
  description = "Enable AWS Shield protection for ALB"
  type        = bool
  default     = true
}



variable "alb_health_check_path" {
  description = "ALB health check path"
  type        = string
  default     = "/api/health"
}

variable "alb_health_check_interval" {
  description = "ALB health check interval in seconds"
  type        = number
  default     = 30
}

variable "alb_health_check_timeout" {
  description = "ALB health check timeout in seconds"
  type        = number
  default     = 5
}

# CloudFront/CDN Configuration
variable "enable_cdn" {
  description = "Enable CloudFront CDN"
  type        = bool
  default     = true
}

variable "cdn_default_ttl" {
  description = "CDN default TTL in seconds"
  type        = number
  default     = 3600
}

variable "cdn_max_ttl" {
  description = "CDN max TTL in seconds"
  type        = number
  default     = 86400
}

# Monitoring Configuration
variable "enable_monitoring" {
  description = "Enable CloudWatch monitoring and alarms"
  type        = bool
  default     = true
}

variable "log_retention_days" {
  description = "CloudWatch log retention in days"
  type        = number
  default     = 30
}

variable "alarm_email" {
  description = "Email for CloudWatch alarms"
  type        = string
  default     = ""
}

# GCP Configuration
variable "gcp_project_id" {
  description = "GCP project ID"
  type        = string
  default     = ""
}

variable "gcp_region" {
  description = "GCP region"
  type        = string
  default     = "us-central1"
}

variable "gke_node_count" {
  description = "GKE initial node count per zone"
  type        = number
  default     = 1
}

variable "gke_machine_type" {
  description = "GKE machine type"
  type        = string
  default     = "e2-standard-2"
}

# Tags
variable "tags" {
  description = "Common tags for all resources"
  type        = map(string)
  default = {
    Project = "StellPoker"
    ManagedBy = "Terraform"
  }
}
