use std::collections::HashMap;
use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_costexplorer::Client;
use aws_sdk_costexplorer::config::Region;
use aws_sdk_costexplorer::types::{
    DateInterval, Granularity, GroupDefinition, GroupDefinitionType,
};
use chrono::Utc;

use crate::discover::Resource;

/// Enrich resources with actual hourly cost derived from Cost Explorer.
///
/// Groups by INSTANCE_TYPE + SERVICE (available on all accounts). Costs are
/// distributed evenly across resources that share the same (service, instance_type)
/// pair, giving a reasonable per-resource estimate without requiring the paid
/// "Cost Explorer Resource Granularity" feature.
///
/// Cost Explorer is a global service with a single endpoint in us-east-1,
/// so we override the region regardless of which region was scanned.
/// On any failure (permissions, not enabled, no data), logs a warning and
/// returns resources unchanged (hourly_on_demand_cost stays None).
pub async fn enrich(
    config: &SdkConfig,
    mut resources: Vec<Resource>,
    days: u32,
) -> anyhow::Result<Vec<Resource>> {
    // Cost Explorer is only available in us-east-1 — override region on the client config.
    let ce_config = aws_sdk_costexplorer::config::Builder::from(config)
        .region(Region::new("us-east-1"))
        .build();
    let client = Client::from_conf(ce_config);

    let end = Utc::now();
    let start = end - chrono::Duration::days(days as i64);
    let start_str = start.format("%Y-%m-%d").to_string();
    let end_str = end.format("%Y-%m-%d").to_string();
    let lookback_hours = days as f64 * 24.0;

    let result = client
        .get_cost_and_usage()
        .time_period(
            DateInterval::builder()
                .start(&start_str)
                .end(&end_str)
                .build()
                .context("building DateInterval")?,
        )
        .granularity(Granularity::Daily)
        .group_by(
            GroupDefinition::builder()
                .r#type(GroupDefinitionType::Dimension)
                .key("INSTANCE_TYPE")
                .build(),
        )
        .group_by(
            GroupDefinition::builder()
                .r#type(GroupDefinitionType::Dimension)
                .key("SERVICE")
                .build(),
        )
        .metrics("UnblendedCost")
        .send()
        .await;

    let resp = match result {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Warning: Cost Explorer unavailable — saving estimates will be omitted. ({e:#})");
            return Ok(resources);
        }
    };

    // Sum total spend per (instance_type, service) over the lookback window.
    // Keys order from the API matches the group_by order: [instance_type, service].
    let mut cost_by_type: HashMap<(String, String), f64> = HashMap::new();
    for time_result in resp.results_by_time.unwrap_or_default() {
        for group in time_result.groups.unwrap_or_default() {
            let mut keys = group.keys.unwrap_or_default().into_iter();
            let instance_type = keys.next().unwrap_or_default();
            let service = keys.next().unwrap_or_default();
            let amount: f64 = group
                .metrics
                .unwrap_or_default()
                .get("UnblendedCost")
                .and_then(|m| m.amount.as_deref())
                .and_then(|a| a.parse().ok())
                .unwrap_or(0.0);
            *cost_by_type.entry((instance_type, service)).or_insert(0.0) += amount;
        }
    }

    // Count how many discovered resources share each (instance_type, service) pair
    // so we can distribute the cost evenly among them.
    let mut count_by_type: HashMap<(String, String), usize> = HashMap::new();
    for r in &resources {
        if let Some(itype) = &r.instance_type {
            let svc = ce_service_name(&r.kind);
            *count_by_type.entry((itype.clone(), svc.to_string())).or_insert(0) += 1;
        }
    }

    // Derive effective hourly rate and attach to each resource.
    for resource in &mut resources {
        if let Some(itype) = &resource.instance_type {
            let svc = ce_service_name(&resource.kind);
            let key = (itype.clone(), svc.to_string());
            if let (Some(&total), Some(&count)) =
                (cost_by_type.get(&key), count_by_type.get(&key))
            {
                if count > 0 && total > 0.0 {
                    resource.hourly_on_demand_cost = Some(total / count as f64 / lookback_hours);
                }
            }
        }
    }

    Ok(resources)
}

/// Map ResourceKind to the AWS service name as it appears in Cost Explorer.
fn ce_service_name(kind: &crate::discover::ResourceKind) -> &'static str {
    use crate::discover::ResourceKind;
    match kind {
        ResourceKind::EC2 => "Amazon Elastic Compute Cloud - Compute",
        ResourceKind::ECS => "Amazon Elastic Container Service",
        ResourceKind::RDS => "Amazon Relational Database Service",
        ResourceKind::NatGateway => "Amazon Virtual Private Cloud",
    }
}
