# TODO - #349 Add rate-limited public API gateway for coordinator

- [x] Add AWS edge/gateway layer in front of coordinator: ALB HTTPS termination

- [ ] Add AWS WAFv2 WebACL with:
  - [ ] IP allowlisting (default deny)
  - [ ] rate limiting (WAF rate-based rule)
  - [ ] managed rule groups for common exploits/bots
- [ ] (Optional/where supported) Add AWS Shield association for DDoS protection
- [ ] Enable WAF/ALB request logging (CloudWatch/S3)
- [ ] Wire CloudFront (if enabled) to use viewer protocol redirect-to-https and keep forwarding
- [ ] Run terraform fmt/validate and provide deployment notes

