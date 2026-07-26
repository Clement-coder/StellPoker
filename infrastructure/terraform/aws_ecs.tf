# AWS ECS Cluster Configuration

# ECS Cluster
resource "aws_ecs_cluster" "main" {
  name = var.ecs_cluster_name

  setting {
    name  = "containerInsights"
    value = var.enable_monitoring ? "enabled" : "disabled"
  }

  tags = {
    Name = var.ecs_cluster_name
  }
}

# ECS Cluster Capacity Providers
resource "aws_ecs_cluster_capacity_providers" "main" {
  cluster_name = aws_ecs_cluster.main.name

  capacity_providers = ["FARGATE", "FARGATE_SPOT"]

  default_capacity_provider_strategy {
    base              = 1
    weight            = 100
    capacity_provider = "FARGATE"
  }

  default_capacity_provider_strategy {
    weight            = 30
    capacity_provider = "FARGATE_SPOT"
  }
}

# CloudWatch Log Group for ECS
resource "aws_cloudwatch_log_group" "ecs" {
  name              = "/ecs/${var.ecs_cluster_name}"
  retention_in_days = var.log_retention_days

  tags = {
    Name = "${var.ecs_cluster_name}-logs"
  }
}

# IAM Role for ECS Task Execution
resource "aws_iam_role" "ecs_task_execution_role" {
  name = "${var.project_name}-ecs-task-execution-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ecs-tasks.amazonaws.com"
      }
    }]
  })
}

resource "aws_iam_role_policy_attachment" "ecs_task_execution_role_policy" {
  role       = aws_iam_role.ecs_task_execution_role.name
  policy_arn = "arn:aws:iam::aws:policy/service-role/AmazonECSTaskExecutionRolePolicy"
}

# IAM Role for ECS Task
resource "aws_iam_role" "ecs_task_role" {
  name = "${var.project_name}-ecs-task-role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Action = "sts:AssumeRole"
      Effect = "Allow"
      Principal = {
        Service = "ecs-tasks.amazonaws.com"
      }
    }]
  })
}

# Allow ECS tasks to access CloudWatch logs
resource "aws_iam_role_policy" "ecs_task_logs" {
  name = "${var.project_name}-ecs-task-logs-policy"
  role = aws_iam_role.ecs_task_role.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = [
        "logs:CreateLogGroup",
        "logs:CreateLogStream",
        "logs:PutLogEvents"
      ]
      Resource = "${aws_cloudwatch_log_group.ecs.arn}:*"
    }]
  })
}

# Application Load Balancer
resource "aws_lb" "main" {
  count               = var.enable_alb ? 1 : 0
  name                = "${var.project_name}-alb"
  internal            = false
  load_balancer_type = "application"
  security_groups     = [data.aws_security_group.alb.id]
  subnets             = data.aws_subnets.public.ids

  enable_deletion_protection = var.environment == "prod"
  enable_http2              = true
  enable_cross_zone_load_balancing = true

  # Access logs are emitted to the ALB-owned logging bucket (if configured by account)
  # or you can add an explicit S3 bucket in a separate change.

  tags = {
    Name = "${var.project_name}-alb"
  }
}


data "aws_security_group" "alb" {
  name   = "${var.project_name}-alb-sg"
  vpc_id = data.aws_vpc.main.id

  depends_on = [aws_security_group.alb]
}

data "aws_vpc" "main" {
  cidr_block = var.vpc_cidr
}

data "aws_subnets" "public" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.main.id]
  }

  filter {
    name   = "tag:Type"
    values = ["Public"]
  }
}

# ALB Target Group for Coordinator
resource "aws_lb_target_group" "coordinator" {
  count           = var.enable_alb ? 1 : 0
  name            = "${var.project_name}-coordinator-tg"
  port            = var.coordinator_container_port
  protocol        = "HTTP"
  vpc_id          = data.aws_vpc.main.id
  target_type     = "ip"
  deregistration_delay = 30

  health_check {
    healthy_threshold   = 2
    unhealthy_threshold = 2
    timeout             = var.alb_health_check_timeout
    interval            = var.alb_health_check_interval
    path                = var.alb_health_check_path
    matcher             = "200-299"
  }

  tags = {
    Name = "${var.project_name}-coordinator-tg"
  }
}

# ALB Listeners (public gateway)
# HTTP (port 80) forwards only when no TLS is configured; otherwise redirect to HTTPS.
resource "aws_lb_listener" "coordinator_http" {
  count             = var.enable_alb ? 1 : 0
  load_balancer_arn = aws_lb.main[0].arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type = var.acm_certificate_arn != "" ? "redirect" : "forward"

    # If TLS is configured, redirect HTTP->HTTPS.
    dynamic "redirect" {
      for_each = var.acm_certificate_arn != "" ? [1] : []
      content {
        port        = "443"
        protocol    = "HTTPS"
        status_code = "HTTP_301"
      }
    }

    # If TLS is not configured, forward HTTP directly.
    dynamic "forward" {
      for_each = var.acm_certificate_arn == "" ? [1] : []
      content {
        target_group_arn = aws_lb_target_group.coordinator[0].arn
      }
    }
  }
}


# HTTPS (port 443) with TLS termination.
resource "aws_lb_listener" "coordinator_https" {
  count             = var.enable_alb && var.acm_certificate_arn != "" ? 1 : 0
  load_balancer_arn = aws_lb.main[0].arn
  port              = 443
  protocol          = "HTTPS"
  ssl_policy        = "ELBSecurityPolicy-TLS13-1-2-2021-06"
  certificate_arn   = var.acm_certificate_arn

  default_action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.coordinator[0].arn
  }
}


# ECS Task Definition for Coordinator
resource "aws_ecs_task_definition" "coordinator" {

  family                   = "${var.project_name}-coordinator"
  network_mode             = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                      = var.coordinator_cpu
  memory                   = var.coordinator_memory
  execution_role_arn       = aws_iam_role.ecs_task_execution_role.arn
  task_role_arn            = aws_iam_role.ecs_task_role.arn

  container_definitions = jsonencode([
    {
      name      = "coordinator"
      image     = var.coordinator_container_image
      essential = true
      portMappings = [
        {
          containerPort = var.coordinator_container_port
          hostPort      = var.coordinator_container_port
          protocol      = "tcp"
        }
      ]
      environment = [
        {
          name  = "MPC_NODE_0"
          value = "http://mpc-node-0:${var.mpc_node_container_port}"
        },
        {
          name  = "MPC_NODE_1"
          value = "http://mpc-node-1:${var.mpc_node_container_port + 1}"
        },
        {
          name  = "MPC_NODE_2"
          value = "http://mpc-node-2:${var.mpc_node_container_port + 2}"
        },
        {
          name  = "RUST_LOG"
          value = var.environment == "prod" ? "info" : "debug"
        }
      ]
      secrets = [
        {
          name      = "DB_PASSWORD"
          valueFrom = aws_secretsmanager_secret_version.db_password.arn
        }
      ]
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"         = aws_cloudwatch_log_group.ecs.name
          "awslogs-region"        = var.aws_region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  tags = {
    Name = "${var.project_name}-coordinator-task"
  }
}

# ECS Service for Coordinator
resource "aws_ecs_service" "coordinator" {
  name            = "${var.project_name}-coordinator-service"
  cluster         = aws_ecs_cluster.main.id
  task_definition = aws_ecs_task_definition.coordinator.arn
  desired_count   = var.coordinator_desired_count
  launch_type     = "FARGATE"


  network_configuration {
    subnets          = data.aws_subnets.private.ids
    security_groups  = [data.aws_security_group.ecs_tasks.id]
    assign_public_ip = false
  }

  load_balancer {
    target_group_arn = var.enable_alb ? aws_lb_target_group.coordinator[0].arn : null
    container_name   = "coordinator"
    container_port   = var.coordinator_container_port
  }

  depends_on = [
    aws_lb_listener.coordinator_http,
    aws_iam_role_policy.ecs_task_logs,
  ]


  tags = {
    Name = "${var.project_name}-coordinator-service"
  }
}

data "aws_subnets" "private" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.main.id]
  }

  filter {
    name   = "tag:Type"
    values = ["Private"]
  }
}

data "aws_security_group" "ecs_tasks" {
  name   = "${var.project_name}-ecs-tasks-sg"
  vpc_id = data.aws_vpc.main.id

  depends_on = [aws_security_group.ecs_tasks]
}

# Auto Scaling for Coordinator Service
resource "aws_appautoscaling_target" "coordinator_target" {
  max_capacity       = var.coordinator_desired_count * 2
  min_capacity       = var.coordinator_desired_count
  resource_id        = "service/${aws_ecs_cluster.main.name}/${aws_ecs_service.coordinator.name}"
  scalable_dimension = "ecs:service:DesiredCount"
  service_namespace  = "ecs"
}

resource "aws_appautoscaling_policy" "coordinator_cpu" {
  policy_name               = "${var.project_name}-coordinator-cpu-scaling"
  policy_type               = "TargetTrackingScaling"
  resource_id               = aws_appautoscaling_target.coordinator_target.resource_id
  scalable_dimension        = aws_appautoscaling_target.coordinator_target.scalable_dimension
  service_namespace         = aws_appautoscaling_target.coordinator_target.service_namespace
  target_tracking_scaling_policy_specification {
    target_value = 70.0
    predefined_metric_specification {
      predefined_metric_type = "ECSServiceAverageCPUUtilization"
    }
    scale_out_cooldown  = 60
    scale_in_cooldown   = 300
  }
}

# Outputs
output "ecs_cluster_id" {
  description = "ECS cluster ID"
  value       = aws_ecs_cluster.main.id
}

output "ecs_cluster_arn" {
  description = "ECS cluster ARN"
  value       = aws_ecs_cluster.main.arn
}

output "alb_dns_name" {
  description = "ALB DNS name"
  value       = var.enable_alb ? aws_lb.main[0].dns_name : ""
}


output "coordinator_service_id" {
  description = "Coordinator service ID"
  value       = aws_ecs_service.coordinator.id
}
