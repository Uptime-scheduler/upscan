use crate::analysis::idle::ResourceAnalysis;
use crate::discover::ResourceKind;

pub fn render(analyses: &[ResourceAnalysis]) -> String {
    let mut out = String::from(
        "id,name,kind,region,instance_type,idle_pct,total_idle_hours,est_saving_mo,pattern,suggested_schedule,active_hrs_outside_schedule\n",
    );
    for a in analyses {
        let kind = match a.resource.kind {
            ResourceKind::EC2 => "EC2", ResourceKind::ECS => "ECS",
            ResourceKind::RDS => "RDS", ResourceKind::NatGateway => "NAT",
        };
        let name = a.resource.name.as_deref().unwrap_or("");
        let itype = a.resource.instance_type.as_deref().unwrap_or("");
        let saving = a.estimated_monthly_saving.map(|s| format!("{s:.2}")).unwrap_or_default();
        let pattern = a.primary_idle_pattern.as_deref().unwrap_or("");
        let schedule = a.suggested_schedule.as_deref().unwrap_or("");
        out.push_str(&format!(
            "{},{},{},{},{},{:.2},{:.2},{},{},{},{}\n",
            csv_escape(&a.resource.id), csv_escape(name), kind,
            csv_escape(&a.resource.region), csv_escape(itype),
            a.idle_percentage, a.total_idle_hours, saving,
            csv_escape(pattern), csv_escape(schedule),
            a.active_hours_outside_schedule,
        ));
    }
    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::idle::ResourceAnalysis;
    use crate::discover::{Resource, ResourceKind};
    use std::collections::HashMap;

    fn make(id: &str) -> ResourceAnalysis {
        ResourceAnalysis {
            resource: Resource {
                id: id.to_string(), name: Some("test".to_string()), kind: ResourceKind::RDS,
                instance_type: Some("db.t3.medium".to_string()), region: "eu-west-1".to_string(),
                tags: HashMap::new(), hourly_on_demand_cost: None,
            },
            idle_windows: vec![], total_idle_hours: 100.0, idle_percentage: 30.0,
            estimated_monthly_saving: Some(89.0), primary_idle_pattern: Some("nights".to_string()),
            suggested_schedule: Some("0800-2000 daily".to_string()),
            idle_classification: None, utilisation_series: vec![], idle_threshold: 5.0,
            active_hours_outside_schedule: 0,
        }
    }

    #[test]
    fn test_csv_has_header() {
        assert!(render(&[]).starts_with("id,name,kind,region"));
    }

    #[test]
    fn test_csv_has_data_row() {
        let output = render(&[make("db-prod")]);
        assert!(output.contains("db-prod") && output.contains("RDS"));
    }
}
