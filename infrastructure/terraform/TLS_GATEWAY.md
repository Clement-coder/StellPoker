# #349 API Gateway (AWS)

This terraform wiring implements an AWS edge in front of the coordinator.

## Components
- ALB HTTPS listener (TLS termination via ACM certificate)
- AWS WAFv2 Web ACL (IP allowlist + rate-based rule + managed rule groups)
- AWS Shield protection for ALB (DDoS)
- WAF logging to CloudWatch Logs (when monitoring enabled)

## Required Terraform variables
- `acm_certificate_arn` (for TLS termination)
- `waf_allowed_ips` (CIDR allowlist)

