use std::collections::HashMap;
use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_ec2::Client;

use super::{Resource, ResourceKind};

/// Discover running EC2 instances, optionally filtered by a tag.
/// tag_filter is (key, value) — only instances matching are returned.
pub async fn discover(
    config: &SdkConfig,
    region: &str,
    tag_filter: Option<(&str, &str)>,
) -> anyhow::Result<Vec<Resource>> {
    let client = Client::new(config);
    let mut resources = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut req = client.describe_instances();

        if let Some(token) = next_token.take() {
            req = req.next_token(token);
        }

        // Include all non-terminal states: running, stopped, stopping, pending.
        // Stopped instances still incur EBS costs and are candidates for scheduling.
        // Terminated/shutting-down are excluded as they no longer accrue charges.
        req = req.filters(
            aws_sdk_ec2::types::Filter::builder()
                .name("instance-state-name")
                .values("running")
                .values("stopped")
                .values("stopping")
                .values("pending")
                .build(),
        );

        if let Some((key, value)) = tag_filter {
            req = req.filters(
                aws_sdk_ec2::types::Filter::builder()
                    .name(format!("tag:{key}"))
                    .values(value)
                    .build(),
            );
        }

        let resp = req.send().await.context("EC2 describe_instances failed")?;

        for reservation in resp.reservations.unwrap_or_default() {
            for instance in reservation.instances.unwrap_or_default() {
                let id = match instance.instance_id {
                    Some(ref id) => id.clone(),
                    None => continue,
                };

                let tags: HashMap<String, String> = instance
                    .tags
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|t| Some((t.key?, t.value?)))
                    .collect();

                let name = tags.get("Name").cloned();
                let instance_type = instance
                    .instance_type
                    .map(|t| t.as_str().to_string());

                resources.push(Resource {
                    id,
                    name,
                    kind: ResourceKind::EC2,
                    instance_type,
                    region: region.to_string(),
                    tags,
                    hourly_on_demand_cost: None,
                });
            }
        }

        next_token = resp.next_token;
        if next_token.is_none() {
            break;
        }
    }

    Ok(resources)
}
