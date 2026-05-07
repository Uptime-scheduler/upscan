# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-07

### Added

- EC2 idle detection via `CPUUtilization` (1-hour CloudWatch buckets, configurable threshold, default 5%)
- ECS service idle detection via `CPUUtilization` per cluster/service
- RDS idle detection using peak-bucket connection floor algorithm on 5-minute `DatabaseConnections` data — correctly handles persistent connection pools that hold connections open between queries
- NAT Gateway idle detection via `BytesOutToDestination` hourly sum — avoids false positives from the sparse `ActiveConnectionCount` metric
- Cost Explorer integration (`ce:GetCostAndUsage`) for actual spend-based saving estimates per resource
- Suggested schedule output per resource, derived from active hour-of-day pattern detection with 2.5% outlier trimming
- Warning when observed active hours fall outside the suggested schedule window
- Terminal table output via `comfy-table`
- JSON, CSV, and HTML output formats (`--output`)
- HTML report includes per-resource utilisation heatmap and Uptime Scheduler onboarding call-to-action
- Multi-region support via `--region` flag
- Tag-based resource filtering via `--tag-filter`
- AWS SSO, named profile, environment variable, and EC2/ECS instance role credential chain support
- Homebrew tap distribution (`uptimescheduler/tap`)
- Pre-built binaries for macOS ARM64, macOS x86_64, Linux x86_64, Linux ARM64, and Windows x86_64

[Unreleased]: https://github.com/uptimescheduler/upscan/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/uptimescheduler/upscan/releases/tag/v0.1.0
