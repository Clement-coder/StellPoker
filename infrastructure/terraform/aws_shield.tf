# AWS Shield (DDoS protection) for the coordinator ALB

# Note: Shield Advanced is required for full coverage of ALB. Shield Standard is global and
# applies automatically to supported resources; here we at least enable the integration if possible.

resource "aws_shield_protection" "alb" {
  count = var.enable_shield && var.enable_alb ? 1 : 0

  name         = "${var.project_name}-shield"
  resource_arn = aws_lb.main[0].arn
}

