# AWS WAFv2 Web ACL for coordinator public API gateway

resource "aws_wafv2_web_acl" "coordinator" {
  name  = "${var.project_name}-coordinator-web-acl"
  scope = "REGIONAL"

  # Allowlist is enforced via an IP set; default is deny.
  default_action {
    block {}
  }

  visibility_config {
    cloudwatch_metrics_enabled = var.enable_monitoring
    metric_name                = "${var.project_name}-coordinator-waf"
    sampled_requests_enabled   = var.enable_monitoring
  }

  # IP allowlist: first rule decides whether request can proceed.
  rule {
    name     = "ip-allowlist"
    priority = 1

    action {
      allow {}
    }

    statement {
      ip_set_reference_statement {
        arn = aws_wafv2_ip_set.allowlist.arn
      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = var.enable_monitoring
      metric_name                = "${var.project_name}-waf-ip-allowlist"
      sampled_requests_enabled   = var.enable_monitoring
    }
  }

  # WAF rate limiting per source IP.
  rule {
    name     = "rate-limit-per-ip"
    priority = 10

    action {
      block {}
    }

    statement {
      rate_based_statement {
        limit              = var.waf_rate_limit_requests
        aggregate_key_type = "IP"

        # Only apply to coordinator API paths.
        scope_down_statement {
          regex_match_statement {
            regex_string = "^/api/.*"
            field_to_match {
              uri_path {}
            }
            text_transformations {
              priority = 0
              type     = "NONE"
            }
          }
        }

      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = var.enable_monitoring
      metric_name                = "${var.project_name}-waf-rate-limit"
      sampled_requests_enabled   = var.enable_monitoring
    }

    # Don't block the entire request if we can't classify it.
  }

  # Managed rules for baseline DDoS/exploit protections.
  rule {
    name     = "managed-common-rules"
    priority = 20

    override_action {
      none {}
    }

    statement {
      managed_rule_group_statement {
        name        = "AWSManagedRulesCommonRuleSet"
        vendor_name = "AWS"
      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = var.enable_monitoring
      metric_name                = "${var.project_name}-waf-managed-common"
      sampled_requests_enabled   = var.enable_monitoring
    }
  }

  rule {
    name     = "managed-bot-control"
    priority = 30

    override_action {
      none {}
    }

    statement {
      managed_rule_group_statement {
        name        = "AWSManagedRulesBotControlRuleSet"
        vendor_name = "AWS"
      }
    }

    visibility_config {
      cloudwatch_metrics_enabled = var.enable_monitoring
      metric_name                = "${var.project_name}-waf-managed-bot"
      sampled_requests_enabled   = var.enable_monitoring
    }
  }
}

resource "aws_wafv2_ip_set" "allowlist" {
  name               = "${var.project_name}-coordinator-allowlist"
  scope              = "REGIONAL"
  ip_address_version = "IPV4"

  # If empty, WAF will block everything (because default_action is block).
  addresses = var.waf_allowed_ips
}

# Attach WAF to ALB
resource "aws_wafv2_web_acl_association" "coordinator_alb" {
  count        = var.enable_alb && var.enable_waf ? 1 : 0
  resource_arn = aws_lb.main[0].arn
  web_acl_arn   = aws_wafv2_web_acl.coordinator.arn
}

# WAF logging
resource "aws_wafv2_web_acl_logging_configuration" "coordinator" {
  count = var.enable_waf && var.enable_monitoring ? 1 : 0

  resource_arn          = aws_wafv2_web_acl.coordinator.arn
  log_destination_configs = [aws_cloudwatch_log_group.waf[0].arn]
}

resource "aws_cloudwatch_log_group" "waf" {
  count             = var.enable_waf && var.enable_monitoring ? 1 : 0
  name              = "/waf/${var.project_name}-coordinator"
  retention_in_days = var.log_retention_days
}

