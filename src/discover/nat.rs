use std::collections::HashMap;
use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_ec2::Client;

use super::{Resource, ResourceKind};

pub async fn discover(
    config: &SdkConfig,
    region: &str,
    tag_filter: Option<(&str, &str)>,
) -> anyhow::Result<Vec<Resource>> {
    let client = Client::new(config);
    let mut resources = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut req = client
            .describe_nat_gateways()
            .filter(
                aws_sdk_ec2::types::Filter::builder()
                    .name("state")
                    .values("available")
                    .build(),
            );

        if let Some(t) = next_token.take() {
            req = req.next_token(t);
        }

        let resp = req
            .send()
            .await
            .context("EC2 describe_nat_gateways failed")?;

        for nat in resp.nat_gateways.unwrap_or_default() {
            let id = match nat.nat_gateway_id {
                Some(ref id) => id.clone(),
                None => continue,
            };

            let tags: HashMap<String, String> = nat
                .tags
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

            let name = tags.get("Name").cloned();

            resources.push(Resource {
                id,
                name,
                kind: ResourceKind::NatGateway,
                instance_type: None,
                region: region.to_string(),
                tags,
                hourly_on_demand_cost: None,
            });
        }

        next_token = resp.next_token;
        if next_token.is_none() {
            break;
        }
    }

    Ok(resources)
}
