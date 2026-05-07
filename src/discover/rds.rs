use std::collections::HashMap;
use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_rds::Client;

use super::{Resource, ResourceKind};

/// Discover available RDS DB instances, optionally filtered by a tag.
///
/// Uses the RDS `tag-value` filter as a server-side pre-filter when tag_filter
/// is provided; client-side key matching is applied afterwards since RDS does
/// not support filtering by tag key natively.
/// Pagination uses `marker` (not `next_token`).
pub async fn discover(
    config: &SdkConfig,
    region: &str,
    tag_filter: Option<(&str, &str)>,
) -> anyhow::Result<Vec<Resource>> {
    let client = Client::new(config);
    let mut resources = Vec::new();
    let mut marker: Option<String> = None;

    loop {
        let mut req = client.describe_db_instances();
        if let Some(m) = marker.take() {
            req = req.marker(m);
        }

        // Server-side pre-filter by tag value when available.
        // RDS does not support filtering by tag key, so client-side key
        // matching is still applied below.
        if let Some((_key, value)) = tag_filter {
            req = req.filters(
                aws_sdk_rds::types::Filter::builder()
                    .name("tag-value")
                    .values(value)
                    .build(),
            );
        }

        let resp = req
            .send()
            .await
            .context("RDS describe_db_instances failed")?;

        for db in resp.db_instances.unwrap_or_default() {
            // Skip terminal/unrecoverable states only. Include stopped/stopping/starting
            // since those instances still incur costs and are scheduling candidates.
            let status = db.db_instance_status.as_deref().unwrap_or("");
            if matches!(status, "deleting" | "deleted" | "failed" | "incompatible-restore" | "restore-error") {
                continue;
            }

            let id = match db.db_instance_identifier {
                Some(ref id) => id.clone(),
                None => continue,
            };

            let tags: HashMap<String, String> = db
                .tag_list
                .unwrap_or_default()
                .into_iter()
                .filter_map(|t| Some((t.key?, t.value?)))
                .collect();

            // Client-side tag filter
            if let Some((key, value)) = tag_filter {
                if tags.get(key).map(|v| v.as_str()) != Some(value) {
                    continue;
                }
            }

            let instance_type = db.db_instance_class;

            resources.push(Resource {
                name: Some(id.clone()),
                id,
                kind: ResourceKind::RDS,
                instance_type,
                region: region.to_string(),
                tags,
                hourly_on_demand_cost: None,
            });
        }

        marker = resp.marker;
        if marker.is_none() {
            break;
        }
    }

    Ok(resources)
}
