use std::collections::HashMap;
use anyhow::Context;
use aws_config::SdkConfig;
use aws_sdk_ecs::Client;

use super::{Resource, ResourceKind};

/// Discover ECS services across all clusters.
/// Resource.id = "cluster-name/service-name".
///
/// Note: ECS does not support server-side tag filtering on describe_services,
/// so tag_filter is applied client-side after all services are described.
/// All services are fetched from the API regardless of the filter value.
pub async fn discover(
    config: &SdkConfig,
    region: &str,
    tag_filter: Option<(&str, &str)>,
) -> anyhow::Result<Vec<Resource>> {
    let client = Client::new(config);
    let mut resources = Vec::new();

    // List all cluster ARNs
    let mut cluster_arns: Vec<String> = Vec::new();
    let mut next_token: Option<String> = None;
    loop {
        let mut req = client.list_clusters();
        if let Some(t) = next_token.take() {
            req = req.next_token(t);
        }
        let resp = req.send().await.context("ECS list_clusters failed")?;
        cluster_arns.extend(resp.cluster_arns.unwrap_or_default());
        next_token = resp.next_token;
        if next_token.is_none() {
            break;
        }
    }

    for cluster_arn in &cluster_arns {
        // Extract short cluster name from ARN: arn:aws:ecs:...:cluster/NAME
        let cluster_name = cluster_arn
            .split('/')
            .last()
            .unwrap_or(cluster_arn.as_str())
            .to_string();

        // List services in this cluster
        let mut service_arns: Vec<String> = Vec::new();
        let mut next_token: Option<String> = None;
        loop {
            let mut req = client.list_services().cluster(cluster_arn.as_str());
            if let Some(t) = next_token.take() {
                req = req.next_token(t);
            }
            let resp = req.send().await.context("ECS list_services failed")?;
            service_arns.extend(resp.service_arns.unwrap_or_default());
            next_token = resp.next_token;
            if next_token.is_none() {
                break;
            }
        }

        if service_arns.is_empty() {
            continue;
        }

        // Describe up to 10 services at a time (API limit)
        for chunk in service_arns.chunks(10) {
            let mut req = client.describe_services().cluster(cluster_arn.as_str());
            for arn in chunk {
                req = req.services(arn.as_str());
            }
            let resp = req
                .send()
                .await
                .context("ECS describe_services failed")?;

            for svc in resp.services.unwrap_or_default() {
                let service_name = match &svc.service_name {
                    Some(n) => n.clone(),
                    None => continue,
                };

                let tags: HashMap<String, String> = svc
                    .tags
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|t| Some((t.key?, t.value?)))
                    .collect();

                // Apply tag filter client-side
                if let Some((key, value)) = tag_filter {
                    if tags.get(key).map(|v| v.as_str()) != Some(value) {
                        continue;
                    }
                }

                resources.push(Resource {
                    id: format!("{cluster_name}/{service_name}"),
                    name: Some(service_name),
                    kind: ResourceKind::ECS,
                    instance_type: None,
                    region: region.to_string(),
                    tags,
                    hourly_on_demand_cost: None,
                });
            }
        }
    }

    Ok(resources)
}
