use anyhow::Context;
use crate::analysis::idle::ResourceAnalysis;

pub fn render(analyses: &[ResourceAnalysis]) -> anyhow::Result<String> {
    serde_json::to_string_pretty(analyses).context("JSON serialisation failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::idle::ResourceAnalysis;
    use crate::discover::{Resource, ResourceKind};
    use std::collections::HashMap;

    fn empty_analysis(id: &str) -> ResourceAnalysis {
        ResourceAnalysis {
            resource: Resource {
                id: id.to_string(), name: None, kind: ResourceKind::EC2,
                instance_type: None, region: "us-east-1".to_string(),
                tags: HashMap::new(), hourly_on_demand_cost: None,
            },
            idle_windows: vec![], total_idle_hours: 0.0, idle_percentage: 0.0,
            estimated_monthly_saving: None, primary_idle_pattern: None,
            suggested_schedule: None, idle_classification: None, utilisation_series: vec![], idle_threshold: 5.0,
            active_hours_outside_schedule: 0,
        }
    }

    #[test]
    fn test_json_is_valid() {
        let json = render(&[empty_analysis("i-001")]).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
    }

    #[test]
    fn test_json_contains_resource_id() {
        let json = render(&[empty_analysis("i-special-id")]).unwrap();
        assert!(json.contains("i-special-id"));
    }
}
