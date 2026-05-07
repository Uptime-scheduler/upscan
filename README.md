# upscan

**Find idle AWS resources. Quantify the waste. Schedule them away.**

`upscan` scans your AWS account for idle EC2, ECS, RDS, and NAT Gateway resources, estimates how much each is costing you, and suggests an on/off schedule to eliminate the waste. It is a companion tool to [Uptime Scheduler](https://uptimescheduler.com), a SaaS product that automates resource scheduling across AWS accounts.

---

## What it looks like

```
Discovering resources in eu-west-2…
  EC2: 2 instance(s)
  ECS: 1 service(s)
  RDS: 1 instance(s)
  NAT: 1 gateway(s)
Fetching Cost Explorer data…

 Resource                    Type         Idle %   Est. Cost/mo   Saving/mo   Suggested Schedule
─────────────────────────────────────────────────────────────────────────────────────────────────────────
 i-0a1b2c3d4e5f (staging)   EC2 t3.med   94 %     $32.48         $30.53      Mon–Fri 08:00–18:00
 api-service (staging)       ECS Fargate  87 %     $18.20         $15.83      Mon–Fri 09:00–17:00
 db-staging (postgres14)     RDS t3.med   91 %     $55.12         $50.16      Mon–Fri 08:00–19:00
 nat-0f9e8d7c6b (eu-west-2)  NAT GW       78 %     $41.00         $31.98      Mon–Fri 07:00–20:00

Total potential saving: $128.50/month

→ Reduce this bill with Uptime Scheduler: https://uptimescheduler.com
```

---

## Installation

### Homebrew

```sh
brew install uptimescheduler/tap/upscan
```

### Direct download

**macOS (Apple Silicon)**
```sh
curl -Lo upscan https://github.com/uptimescheduler/upscan/releases/latest/download/upscan-aarch64-apple-darwin
chmod +x upscan && sudo mv upscan /usr/local/bin/
```

**macOS (Intel)**
```sh
curl -Lo upscan https://github.com/uptimescheduler/upscan/releases/latest/download/upscan-x86_64-apple-darwin
chmod +x upscan && sudo mv upscan /usr/local/bin/
```

**Linux (x86_64)**
```sh
curl -Lo upscan https://github.com/uptimescheduler/upscan/releases/latest/download/upscan-x86_64-unknown-linux-musl
chmod +x upscan && sudo mv upscan /usr/local/bin/
```

### Cargo

```sh
cargo install upscan
```

---

## Prerequisites

AWS credentials must be configured before running `upscan`. It works with any credential source in the standard AWS credential chain:

- Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`)
- Shared credentials file (`~/.aws/credentials`)
- AWS SSO profiles configured via `aws configure sso` — see the [AWS SSO setup guide](https://docs.aws.amazon.com/cli/latest/userguide/sso-configure-profile-token.html)
- EC2/ECS instance roles and EKS service accounts

If you use AWS SSO, authenticate first with `aws sso login --profile <profile>` then pass `--profile <profile>` to `upscan`.

---

## Usage

```
upscan [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--profile <name>` | AWS default | AWS credentials profile to use |
| `--region <region>` | `eu-west-1` | AWS region to scan |
| `--days <n>` | `30` | Number of days of CloudWatch history to analyse |
| `--threshold <pct>` | `5` | CPU/traffic percentage below which a resource is considered idle |
| `--resources <list>` | `ec2,ecs,rds,nat` | Comma-separated resource types to scan |
| `--output <format>` | `table` | Output format: `table`, `json`, `csv`, or `html` |
| `--tag-filter <k=v>` | — | Filter resources by tag key=value (e.g. `Environment=staging`) |
| `--save <path>` | — | Save report output to a file |
| `--min-idle-hours <n>` | — | Only report resources idle for at least N hours per day on average |

### Examples

**Basic scan of the default region:**
```sh
upscan
```

**Scan a specific region and save an HTML report:**
```sh
upscan --region us-east-1 --output html --save report.html
```

**Scan only RDS and NAT Gateway resources:**
```sh
upscan --resources rds,nat
```

**Filter resources by environment tag:**
```sh
upscan --tag-filter Environment=staging
```

---

## Required IAM permissions

The following read-only IAM policy grants everything `upscan` needs:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Sid": "UpscanReadOnly",
      "Effect": "Allow",
      "Action": [
        "ec2:DescribeInstances",
        "ec2:DescribeNatGateways",
        "ecs:ListClusters",
        "ecs:ListServices",
        "ecs:DescribeServices",
        "rds:DescribeDBInstances",
        "cloudwatch:GetMetricStatistics",
        "cloudwatch:GetMetricData",
        "ce:GetCostAndUsage"
      ],
      "Resource": "*"
    }
  ]
}
```

`upscan` never performs any write, stop, start, modify, or delete operations.

---

## How it works

**EC2 & ECS** — `upscan` pulls `CPUUtilization` from CloudWatch in 1-hour buckets over the configured look-back window and flags any instance or service where peak CPU remains below the idle threshold (default 5%) for the majority of hours.

**RDS** — Instead of relying on average CPU (which application-layer connection pools inflate), `upscan` uses a peak-bucket connection floor algorithm on 5-minute `DatabaseConnections` data. It identifies the lowest observed peak within each hour and treats the instance as idle when that floor is zero — meaning no real query traffic arrived during that bucket, even if a persistent pool held connections open throughout.

**NAT Gateway** — `upscan` sums `BytesOutToDestination` in hourly buckets. An hour with zero bytes out is treated as idle. This avoids the false positives that the sparse `ActiveConnectionCount` metric produces for NAT Gateways carrying only keep-alive TCP traffic.

For all resource types, `upscan` fits the idle/active pattern to suggest an on/off schedule and trims the top and bottom 2.5% of hours as outliers before computing the schedule window.

---

## License

[BUSL-1.1](LICENSE) — free to use to scan your own infrastructure.
