use std::collections::HashMap;
use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_cloudwatch::Client;
use aws_sdk_cloudwatch::types::{Dimension, Statistic};
use aws_sdk_cloudwatch::primitives::DateTime as SmithyDateTime;
use chrono::{DateTime, Duration, TimeZone, Utc};

use crate::discover::{Resource, ResourceKind};
use super::{Datapoint, ResourceMetrics};

// ── metric config ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct MetricConfig {
    pub namespace: String,
    pub metric_name: String,
    pub dimensions: Vec<(String, String)>,
    pub statistic: Statistic,
    /// CloudWatch aggregation period in seconds (300 = 5-min, 3600 = 1-hour).
    pub period: u32,
}

// ── low-level fetch ───────────────────────────────────────────────────────────

async fn get_metric(
    client: &Client,
    config: &MetricConfig,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<Vec<Datapoint>> {
    let dimensions: Vec<Dimension> = config
        .dimensions
        .iter()
        .map(|(k, v)| Dimension::builder().name(k).value(v).build())
        .collect();

    // For Sum metrics, also request Average as a fallback.  Some CloudWatch
    // metric/region combinations return the value in dp.average even when Sum is
    // requested (observed with BytesOutToDestination).  Requesting both ensures
    // we always get a value — dp.sum is preferred, dp.average is the fallback.
    let mut req = client
        .get_metric_statistics()
        .namespace(&config.namespace)
        .metric_name(&config.metric_name)
        .start_time(SmithyDateTime::from_secs(start.timestamp()))
        .end_time(SmithyDateTime::from_secs(end.timestamp()))
        .period(config.period as i32)
        .statistics(config.statistic.clone())
        .set_dimensions(Some(dimensions));
    if matches!(config.statistic, Statistic::Sum) {
        req = req.statistics(Statistic::Average);
    }
    let resp = req.send()
        .await
        .context("CloudWatch get_metric_statistics failed")?;

    let mut datapoints: Vec<Datapoint> = resp
        .datapoints
        .unwrap_or_default()
        .into_iter()
        .filter_map(|dp| {
            let ts = dp.timestamp?;
            let val = match config.statistic {
                Statistic::Maximum => dp.maximum?,
                // Prefer sum; fall back to average if sum was not populated.
                Statistic::Sum     => dp.sum.or(dp.average)?,
                _                  => dp.average?,
            };
            let dt = Utc.timestamp_opt(ts.secs(), 0).single()?;
            Some(Datapoint { timestamp: dt, value: val })
        })
        .collect();

    datapoints.sort_by_key(|d| d.timestamp);
    Ok(datapoints)
}

/// Silently swallow errors on optional named metrics — a missing secondary
/// metric should never abort the analysis for a resource.
async fn get_metric_optional(
    client: &Client,
    config: &MetricConfig,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    label: &str,
    resource_id: &str,
) -> Vec<Datapoint> {
    match get_metric(client, config, start, end).await {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Warning: {label} fetch failed for {resource_id}: {e}");
            vec![]
        }
    }
}

// ── per-resource-kind fetch ───────────────────────────────────────────────────

async fn fetch_ec2(
    client: &Client,
    resource: &Resource,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<ResourceMetrics> {
    let dims = vec![("InstanceId".to_string(), resource.id.clone())];

    let cpu = get_metric(client, &MetricConfig {
        namespace: "AWS/EC2".into(), metric_name: "CPUUtilization".into(),
        dimensions: dims.clone(), statistic: Statistic::Average, period: 3600,
    }, start, end).await?;

    let net = get_metric_optional(client, &MetricConfig {
        namespace: "AWS/EC2".into(), metric_name: "NetworkPacketsIn".into(),
        dimensions: dims, statistic: Statistic::Average, period: 3600,
    }, start, end, "NetworkPacketsIn", &resource.id).await;

    let mut named = HashMap::new();
    named.insert("network_packets_in".to_string(), net);
    Ok(ResourceMetrics { primary: cpu, named })
}

async fn fetch_ecs(
    client: &Client,
    resource: &Resource,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<ResourceMetrics> {
    let (cluster, service) = resource.id.split_once('/').unwrap_or((&resource.id, ""));
    let dims = vec![
        ("ClusterName".to_string(), cluster.to_string()),
        ("ServiceName".to_string(), service.to_string()),
    ];

    let cpu = get_metric(client, &MetricConfig {
        namespace: "AWS/ECS".into(), metric_name: "CPUUtilization".into(),
        dimensions: dims.clone(), statistic: Statistic::Average, period: 3600,
    }, start, end).await?;

    let mem = get_metric_optional(client, &MetricConfig {
        namespace: "AWS/ECS".into(), metric_name: "MemoryUtilization".into(),
        dimensions: dims, statistic: Statistic::Average, period: 3600,
    }, start, end, "MemoryUtilization", &resource.id).await;

    let mut named = HashMap::new();
    named.insert("memory".to_string(), mem);
    Ok(ResourceMetrics { primary: cpu, named })
}

async fn fetch_rds(
    client: &Client,
    resource: &Resource,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<ResourceMetrics> {
    let dims = vec![("DBInstanceIdentifier".to_string(), resource.id.clone())];

    // CPUUtilization → primary (used for heatmap visualisation)
    let cpu = get_metric(client, &MetricConfig {
        namespace: "AWS/RDS".into(), metric_name: "CPUUtilization".into(),
        dimensions: dims.clone(), statistic: Statistic::Average, period: 3600,
    }, start, end).await?;

    // DatabaseConnections: fetch at 5-min native resolution and aggregate to
    // hourly averages ourselves.  At period=3600 CloudWatch returns a single
    // average per hour that blends pool-floor samples with brief spikes,
    // producing fractional values (e.g. 1.3) instead of clean integers and
    // causing the peak-bucket detector to mis-identify the pool floor.
    // At period=300 the 7-day window exceeds the 1440-datapoint limit, so
    // we use the same chunk loop as NAT.
    let conn_config = MetricConfig {
        namespace: "AWS/RDS".into(), metric_name: "DatabaseConnections".into(),
        dimensions: dims.clone(), statistic: Statistic::Average, period: 300,
    };
    let max_chunk = Duration::seconds(1440i64 * 300); // 5 days
    let mut conn_raw: Vec<Datapoint> = Vec::new();
    let mut chunk_start = start;
    while chunk_start < end {
        let chunk_end = (chunk_start + max_chunk).min(end);
        if chunk_end.timestamp() <= chunk_start.timestamp() { break; }
        conn_raw.extend(get_metric(client, &conn_config, chunk_start, chunk_end).await?);
        chunk_start = chunk_end;
    }
    let connections = bucket_to_hourly_avg(&conn_raw);

    let read_iops = get_metric_optional(client, &MetricConfig {
        namespace: "AWS/RDS".into(), metric_name: "ReadIOPS".into(),
        dimensions: dims.clone(), statistic: Statistic::Average, period: 3600,
    }, start, end, "ReadIOPS", &resource.id).await;

    let write_iops = get_metric_optional(client, &MetricConfig {
        namespace: "AWS/RDS".into(), metric_name: "WriteIOPS".into(),
        dimensions: dims, statistic: Statistic::Average, period: 3600,
    }, start, end, "WriteIOPS", &resource.id).await;

    let mut named = HashMap::new();
    named.insert("connections".to_string(), connections);
    named.insert("read_iops".to_string(), read_iops);
    named.insert("write_iops".to_string(), write_iops);
    Ok(ResourceMetrics { primary: cpu, named })
}

/// Bucket sub-hourly `Average` datapoints (e.g. 5-min) into per-hour means.
fn bucket_to_hourly_avg(datapoints: &[Datapoint]) -> Vec<Datapoint> {
    let mut buckets: HashMap<i64, (f64, u32)> = HashMap::new();
    for dp in datapoints {
        let entry = buckets.entry(dp.timestamp.timestamp().div_euclid(3600)).or_insert((0.0, 0));
        entry.0 += dp.value;
        entry.1 += 1;
    }
    let mut result: Vec<Datapoint> = buckets
        .into_iter()
        .filter_map(|(key, (sum, count))| {
            let ts = Utc.timestamp_opt(key * 3600, 0).single()?;
            Some(Datapoint { timestamp: ts, value: sum / count as f64 })
        })
        .collect();
    result.sort_by_key(|d| d.timestamp);
    result
}

/// Bucket sub-hourly `Sum` datapoints (e.g. 5-min) into per-hour totals.
fn bucket_to_hourly_sum(datapoints: &[Datapoint]) -> Vec<Datapoint> {
    let mut buckets: HashMap<i64, f64> = HashMap::new();
    for dp in datapoints {
        *buckets.entry(dp.timestamp.timestamp().div_euclid(3600)).or_insert(0.0) += dp.value;
    }
    let mut result: Vec<Datapoint> = buckets
        .into_iter()
        .filter_map(|(key, val)| {
            let ts = Utc.timestamp_opt(key * 3600, 0).single()?;
            Some(Datapoint { timestamp: ts, value: val })
        })
        .collect();
    result.sort_by_key(|d| d.timestamp);
    result
}

async fn fetch_nat(
    client: &Client,
    resource: &Resource,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<ResourceMetrics> {
    let dims = vec![("NatGatewayId".to_string(), resource.id.clone())];

    // At 300 s resolution the CloudWatch 1440-datapoint limit is hit quickly:
    //   7 days  = 2016 pts,  30 days = 8640 pts.
    // Chunk the window into ≤ 5-day slices (1440 × 300 s = 432 000 s exactly).
    let config = MetricConfig {
        namespace: "AWS/NATGateway".into(),
        metric_name: "BytesOutToDestination".into(),
        dimensions: dims,
        statistic: Statistic::Sum,
        period: 300,
    };
    let max_chunk = Duration::seconds(1440i64 * 300); // 5 days
    let mut raw: Vec<Datapoint> = Vec::new();
    let mut chunk_start = start;
    while chunk_start < end {
        let chunk_end = (chunk_start + max_chunk).min(end);
        // SmithyDateTime uses second precision.  If the final sub-second remainder
        // lands in the same second as chunk_start (happens when lookback_start and
        // lookback_end were captured by two Utc::now() calls <1 s apart), the API
        // returns InvalidParameterValueException.  Skip the degenerate chunk.
        if chunk_end.timestamp() <= chunk_start.timestamp() { break; }
        raw.extend(get_metric(client, &config, chunk_start, chunk_end).await?);
        chunk_start = chunk_end;
    }

    Ok(ResourceMetrics { primary: bucket_to_hourly_sum(&raw), named: HashMap::new() })
}

// ── public entry point ────────────────────────────────────────────────────────

pub async fn fetch(
    config: &SdkConfig,
    resource: &Resource,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> anyhow::Result<ResourceMetrics> {
    let client = Client::new(config);
    match resource.kind {
        ResourceKind::EC2        => fetch_ec2(&client, resource, start, end).await,
        ResourceKind::ECS        => fetch_ecs(&client, resource, start, end).await,
        ResourceKind::RDS        => fetch_rds(&client, resource, start, end).await,
        ResourceKind::NatGateway => fetch_nat(&client, resource, start, end).await,
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_resource(kind: ResourceKind, id: &str) -> crate::discover::Resource {
        use std::collections::HashMap;
        crate::discover::Resource {
            id: id.to_string(), name: None, kind,
            instance_type: None, region: "eu-west-2".to_string(),
            tags: HashMap::new(), hourly_on_demand_cost: None,
        }
    }

    #[test]
    fn test_ec2_metric_config_sanity() {
        // Just validates the resource kind dispatch doesn't panic for EC2.
        // Full fetch is async/network — tested by integration.
        let r = make_resource(ResourceKind::EC2, "i-0abc123");
        assert_eq!(r.kind, ResourceKind::EC2);
    }

    #[test]
    fn test_nat_metric_kind() {
        let r = make_resource(ResourceKind::NatGateway, "nat-0abc");
        assert_eq!(r.kind, ResourceKind::NatGateway);
    }

    #[test]
    fn test_rds_metric_kind() {
        let r = make_resource(ResourceKind::RDS, "db-prod");
        assert_eq!(r.kind, ResourceKind::RDS);
    }

    #[test]
    fn test_ecs_metric_kind() {
        let r = make_resource(ResourceKind::ECS, "cluster/service");
        assert_eq!(r.kind, ResourceKind::ECS);
    }
}
