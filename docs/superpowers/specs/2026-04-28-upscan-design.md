# upscan — Design Spec

**Date:** 2026-04-28
**Status:** Approved
**Location:** `app/tools/upscan/`

---

## Overview

`upscan` is a Rust CLI tool that analyses AWS resource usage via CloudWatch metrics, detects idle periods, and generates reports estimating potential cost savings from scheduling (stopping/starting resources during idle windows). The report concludes with a CTA directing users to Uptime Scheduler.

---

## Location

`app/tools/upscan/` — alongside the existing `pricing-extractor` tool.

---

## Project Structure

```
upscan/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs              # tokio::main entry; wires discover → pricing → metrics → analysis → report
    ├── cli.rs               # clap derive Args struct
    ├── auth.rs              # aws-config SdkConfig builder
    ├── discover/
    │   ├── mod.rs           # ResourceKind enum, Resource struct, tag-filter helpers
    │   ├── ec2.rs
    │   ├── ecs.rs
    │   ├── rds.rs
    │   └── nat.rs
    ├── metrics/
    │   ├── mod.rs
    │   └── cloudwatch.rs    # concurrent fetch via futures::join_all + indicatif progress
    ├── analysis/
    │   ├── mod.rs
    │   └── idle.rs          # detect_idle_windows, infer_pattern, saving estimation
    ├── pricing/
    │   ├── mod.rs
    │   └── cost_explorer.rs # get_cost_and_usage → effective hourly rate per resource
    └── report/
        ├── mod.rs
        ├── table.rs         # comfy-table terminal output
        ├── json.rs          # serde serialisation
        ├── html.rs          # self-contained HTML with heatmap + CTA
        └── csv.rs           # flat CSV for spreadsheet import
```

---

## CLI Interface

```
upscan [OPTIONS]

Options:
  --profile <PROFILE>         AWS profile name (falls back to env/SSO/instance role)
  --region <REGION>           AWS region [default: us-east-1]
  --days <DAYS>               Lookback window in days [default: 30]
  --threshold <VALUE>         Idle threshold: % for EC2/ECS/RDS, bytes/hour for NAT [default: 5]
  --resources <LIST>          Comma-separated: ec2,ecs,rds,nat [default: all]
  --output <FORMAT>           table|json|csv|html [default: table]
  --tag-filter <K=V>          Only include resources matching this tag
  --save <PATH>               Write report to file (stdout if omitted)
  --min-idle-hours <HOURS>    Minimum contiguous idle block to report [default: 2]
```

`--resources` is parsed into `HashSet<ResourceKind>` to control which discovery modules run.

---

## Dependencies

```toml
[dependencies]
aws-config = "1"
aws-sdk-cloudwatch = "1"
aws-sdk-ec2 = "1"
aws-sdk-ecs = "1"
aws-sdk-rds = "1"
aws-sdk-costexplorer = "1"
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
comfy-table = "7"
indicatif = "0.17"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
futures = "0.3"
```

---

## Data Structures

All public structs derive `Debug`, `Serialize`, `Deserialize`.

```rust
pub enum ResourceKind { EC2, ECS, RDS, NatGateway }

pub struct Resource {
    pub id: String,                           // e.g. "i-0abc123ef" or "cluster/service"
    pub name: Option<String>,                 // from Name tag, or service name for ECS
    pub kind: ResourceKind,
    pub instance_type: Option<String>,
    pub region: String,
    pub tags: HashMap<String, String>,
    pub hourly_on_demand_cost: Option<f64>,   // populated by pricing module
}

pub struct IdleWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub duration_hours: f64,
    pub avg_utilisation: f64,
}

pub struct ResourceAnalysis {
    pub resource: Resource,
    pub idle_windows: Vec<IdleWindow>,
    pub total_idle_hours: f64,
    pub idle_percentage: f64,
    pub estimated_monthly_saving: Option<f64>, // None if hourly cost unavailable
    pub primary_idle_pattern: Option<String>,  // e.g. "nights + weekends"
}
```

**ECS identity:** `Resource.id = "cluster-name/service-name"`. CloudWatch dimensions split this into `ClusterName` + `ServiceName` at query time.

---

## Authentication

`auth.rs` uses `aws-config` with the standard credential chain:
- If `--profile` is provided: `aws_config::from_env().profile_name(profile).load().await`
- Otherwise: default chain (env vars → SSO → instance role)
- Hard error with clear message if credentials cannot be resolved

---

## Resource Discovery

Each module returns `Vec<Resource>`. Tag filtering (`--tag-filter K=V`) is applied:
- **EC2 / RDS**: passed as an API filter in the describe call
- **ECS / NAT**: fetched in full, then filtered client-side

| Module  | API call                                       | Skip condition                         |
|---------|------------------------------------------------|----------------------------------------|
| ec2.rs  | `describe_instances`                           | state = terminated or stopped with no running hours |
| ecs.rs  | `list_clusters` → `list_services` → `describe_services` | none |
| rds.rs  | `describe_db_instances`                        | status ≠ `available`                   |
| nat.rs  | `describe_nat_gateways`                        | state ≠ `available`                    |

---

## Metrics Collection

`cloudwatch.rs` calls `get_metric_statistics` per resource with:
- Period: 3600 (1-hour granularity)
- StartTime: `now - days`
- EndTime: `now`
- Statistics: `[Average]`

| Kind        | Namespace        | MetricName              | Dimension                               |
|-------------|------------------|-------------------------|-----------------------------------------|
| EC2         | AWS/EC2          | CPUUtilization          | InstanceId = resource.id                |
| EC2         | AWS/EC2          | NetworkIn               | InstanceId = resource.id                |
| ECS         | AWS/ECS          | CPUUtilization          | ClusterName + ServiceName               |
| ECS         | AWS/ECS          | MemoryUtilization       | ClusterName + ServiceName               |
| RDS         | AWS/RDS          | DatabaseConnections     | DBInstanceIdentifier = resource.id      |
| RDS         | AWS/RDS          | CPUUtilization          | DBInstanceIdentifier = resource.id      |
| NatGateway  | AWS/NatGateway   | BytesOutToDestination   | NatGatewayId = resource.id              |

All queries run concurrently via `futures::join_all`. An `indicatif` progress bar tracks completion.

---

## Idle Detection

`detect_idle_windows(datapoints, threshold, min_idle_hours)`:
1. Sort datapoints by timestamp
2. Mark each hour idle if the **primary metric** < `threshold`
3. Merge consecutive idle hours into `IdleWindow` blocks
4. Drop blocks with `duration_hours < min_idle_hours`

**Primary metric per resource kind** (used for idle determination; secondary metrics are stored but not used for the idle/active decision):

| Kind        | Primary metric          | Threshold unit     |
|-------------|-------------------------|--------------------|
| EC2         | CPUUtilization          | % (0–100)          |
| ECS         | CPUUtilization          | % (0–100)          |
| RDS         | CPUUtilization          | % (0–100)          |
| NatGateway  | BytesOutToDestination   | bytes/hour         |

For NAT Gateways, `--threshold` is interpreted as bytes/hour (not %). Default of `5` means < 5 bytes/hour = idle, which is effectively zero traffic. Users scanning NAT gateways should set an appropriate threshold (e.g. `--threshold 1000000` for < 1 MB/hour).

`infer_pattern(windows)`:
- Bucket idle hours by time-of-day and day-of-week across the full lookback
- "nights" if ≥ 60% of idle hours fall between 18:00–08:00 UTC
- "weekends" if ≥ 60% fall on Saturday/Sunday
- Combinations: "nights + weekends", "weekdays only", "nights only"
- `None` if no pattern meets the 60% threshold

**Saving estimate:**
```
estimated_monthly_saving = total_idle_hours × (720 / lookback_hours) × hourly_cost
```
Normalises to a 30-day month regardless of `--days`. If `hourly_on_demand_cost` is `None`, saving shows `"n/a"` and a footer note explains.

---

## Pricing

`cost_explorer.rs`:
- `get_cost_and_usage` grouped by `RESOURCE_ID`, granularity `DAILY`, over the lookback window
- Sum daily costs per resource → divide by total hours = effective hourly rate
- Graceful fallback: if Cost Explorer is unavailable or IAM denies `ce:GetCostAndUsage`, log a warning and set all `hourly_on_demand_cost = None`

---

## Report Output

**table** (default):
```
Resource              Type         Idle periods    Idle %   Est. saving/mo
─────────────────────────────────────────────────────────────────────────
i-0abc123 (t3.large)  EC2          nights+wkends   61%      $47.20
my-api (ecs-svc)      ECS          nights          38%      $12.40
db-prod (db.r6g.xl)   RDS          Sat–Sun         28%      $89.00
─────────────────────────────────────────────────────────────────────────
Total potential saving                                       $148.60/month
→ Reduce this bill with Uptime Scheduler: https://uptimescheduler.com
```

**json** — `Vec<ResourceAnalysis>` via serde → stdout or `--save` path

**csv** — flat rows of `ResourceAnalysis` fields → stdout or `--save` path

**html** — self-contained single file, inline CSS only:
- Header: total saving + scan metadata (account, region, date, lookback days)
- Table matching terminal output
- Per-resource 7-day × 24-hour idle heatmap (green = active, amber/red = idle)
- CTA button → `https://uptimescheduler.com`
- Written to stdout unless `--save` is provided

---

## Error Handling

| Failure                        | Behaviour                                              |
|--------------------------------|--------------------------------------------------------|
| Auth failure                   | Hard error, exit immediately with clear message        |
| Entire resource type discovery fails | Log warning, skip type, continue              |
| Single resource metric fetch fails | Log warning, skip resource, continue             |
| Cost Explorer unavailable      | Log warning, continue without saving estimates         |

End of report: print "Skipped resources" summary listing all errored resources.

---

## Required IAM Permissions

```json
{
  "Effect": "Allow",
  "Action": [
    "ec2:DescribeInstances",
    "ec2:DescribeNatGateways",
    "ecs:ListClusters",
    "ecs:ListServices",
    "ecs:DescribeServices",
    "rds:DescribeDBInstances",
    "cloudwatch:GetMetricStatistics",
    "ce:GetCostAndUsage"
  ],
  "Resource": "*"
}
```

---

## CTA

All report formats conclude with a link to:
`https://uptimescheduler.com`
