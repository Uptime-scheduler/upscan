/// Hardcoded NAT gateway hourly rates by region (gateway-hour only, excl. data processing).
/// Source: AWS public pricing pages. Used as a fallback when Cost Explorer has no cost data
/// for a NAT gateway (CE groups by instance type, which NAT gateways don't have).
///
/// All prices in USD. Updated 2025. For regions not listed, `DEFAULT_RATE` is used.
const DEFAULT_RATE: f64 = 0.048;

pub fn hourly_rate(region: &str) -> f64 {
    match region {
        // US
        "us-east-1" | "us-east-2" | "us-west-1" | "us-west-2" => 0.045,
        // Canada
        "ca-central-1" | "ca-west-1" => 0.048,
        // Europe
        "eu-west-1" | "eu-west-2" | "eu-west-3" => 0.048,
        "eu-central-1" | "eu-central-2" => 0.052,
        "eu-north-1" | "eu-south-1" | "eu-south-2" => 0.052,
        // Asia Pacific
        "ap-southeast-1" | "ap-southeast-2" | "ap-southeast-3" | "ap-southeast-4" => 0.059,
        "ap-northeast-1" | "ap-northeast-2" | "ap-northeast-3" => 0.059,
        "ap-south-1" | "ap-south-2" => 0.052,
        "ap-east-1" => 0.059,
        // South America
        "sa-east-1" => 0.059,
        // Middle East / Africa
        "me-south-1" | "me-central-1" | "af-south-1" => 0.059,
        _ => DEFAULT_RATE,
    }
}

/// Apply NAT gateway pricing to any NAT resource that has no hourly cost set.
/// CE cannot price NAT gateways (they have no instance type dimension), so this
/// fallback ensures saving estimates are always populated for NAT resources.
pub fn apply_fallback_pricing(resources: &mut [crate::discover::Resource]) {
    for r in resources.iter_mut() {
        if r.kind == crate::discover::ResourceKind::NatGateway && r.hourly_on_demand_cost.is_none() {
            r.hourly_on_demand_cost = Some(hourly_rate(&r.region));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_regions() {
        assert_eq!(hourly_rate("us-east-1"), 0.045);
        assert_eq!(hourly_rate("eu-west-2"), 0.048);
        assert_eq!(hourly_rate("ap-southeast-1"), 0.059);
    }

    #[test]
    fn test_unknown_region_uses_default() {
        assert_eq!(hourly_rate("xx-unknown-1"), DEFAULT_RATE);
    }

    #[test]
    fn test_apply_fallback_sets_cost_for_nat() {
        use crate::discover::{Resource, ResourceKind};
        use std::collections::HashMap;

        let mut resources = vec![
            Resource {
                id: "nat-abc".to_string(),
                name: None,
                kind: ResourceKind::NatGateway,
                instance_type: None,
                region: "eu-west-2".to_string(),
                tags: HashMap::new(),
                hourly_on_demand_cost: None,
            },
        ];
        apply_fallback_pricing(&mut resources);
        assert_eq!(resources[0].hourly_on_demand_cost, Some(0.048));
    }

    #[test]
    fn test_apply_fallback_does_not_overwrite_existing_cost() {
        use crate::discover::{Resource, ResourceKind};
        use std::collections::HashMap;

        let mut resources = vec![
            Resource {
                id: "nat-abc".to_string(),
                name: None,
                kind: ResourceKind::NatGateway,
                instance_type: None,
                region: "eu-west-2".to_string(),
                tags: HashMap::new(),
                hourly_on_demand_cost: Some(0.099),
            },
        ];
        apply_fallback_pricing(&mut resources);
        assert_eq!(resources[0].hourly_on_demand_cost, Some(0.099));
    }
}
