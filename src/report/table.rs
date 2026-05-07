use comfy_table::{Cell, Table, presets::UTF8_FULL};
use crate::analysis::idle::ResourceAnalysis;
use crate::discover::ResourceKind;

pub fn render(analyses: &[ResourceAnalysis]) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("Resource"),
        Cell::new("Type"),
        Cell::new("Idle %"),
        Cell::new("Est. saving/mo"),
        Cell::new("Suggested schedule"),
    ]);

    let mut total_saving = 0.0f64;
    let mut has_any_cost = false;

    for a in analyses {
        let resource_label = match (&a.resource.name, &a.resource.instance_type) {
            (Some(name), Some(itype)) => format!("{name} ({itype})"),
            (Some(name), None) => name.clone(),
            (None, _) => a.resource.id.clone(),
        };
        let kind_str = match a.resource.kind {
            ResourceKind::EC2 => "EC2",
            ResourceKind::ECS => "ECS",
            ResourceKind::RDS => "RDS",
            ResourceKind::NatGateway => "NAT",
        };
        let idle_pct = match a.idle_classification.as_deref() {
            Some("pool-only")  => format!("{:.0}% (pool-only)", a.idle_percentage.abs()),
            Some("confirmed")  => format!("{:.0}% (confirmed)", a.idle_percentage.abs()),
            _                  => format!("{:.0}%", a.idle_percentage.abs()),
        };
        let saving_str = match a.estimated_monthly_saving {
            Some(s) => { total_saving += s; has_any_cost = true; format!("${:.2}", s) }
            None => "n/a".to_string(),
        };
        let schedule = match (a.suggested_schedule.as_deref(), a.active_hours_outside_schedule) {
            (Some(s), 0)    => s.to_string(),
            (Some(s), n)    => format!("{s}  ⚠ {n} active hrs outside window"),
            (None,    0)    => "—".to_string(),
            (None,    n)    => format!("—  ⚠ {n} active hrs, no safe schedule"),
        };
        table.add_row(vec![
            Cell::new(resource_label),
            Cell::new(kind_str),
            Cell::new(idle_pct),
            Cell::new(saving_str),
            Cell::new(schedule),
        ]);
    }

    let mut output = table.to_string();
    if has_any_cost && total_saving > 0.0 {
        output.push_str(&format!("\nTotal potential saving: ${:.2}/month\n", total_saving));
    }
    output.push_str("\n⚠  Schedules are a guideline — verify against your workload before applying in production.\n");
    output.push_str("\n→ Reduce this bill with Uptime Scheduler: https://uptimescheduler.com\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::idle::ResourceAnalysis;
    use crate::discover::{Resource, ResourceKind};
    use std::collections::HashMap;

    fn make_analysis(id: &str, kind: ResourceKind, idle_pct: f64, saving: Option<f64>, schedule: Option<&str>) -> ResourceAnalysis {
        ResourceAnalysis {
            resource: Resource {
                id: id.to_string(),
                name: Some(id.to_string()),
                kind,
                instance_type: Some("t3.large".to_string()),
                region: "us-east-1".to_string(),
                tags: HashMap::new(),
                hourly_on_demand_cost: saving.map(|s| s / 720.0),
            },
            idle_windows: vec![],
            total_idle_hours: idle_pct * 7.2,
            idle_percentage: idle_pct,
            estimated_monthly_saving: saving,
            primary_idle_pattern: schedule.map(|_| "nights".to_string()),
            suggested_schedule: schedule.map(|s| s.to_string()),
            idle_classification: None,
            utilisation_series: vec![],
            idle_threshold: 5.0,
            active_hours_outside_schedule: 0,
        }
    }

    #[test]
    fn test_render_includes_resource_id() {
        let output = render(&[make_analysis("i-0abc123", ResourceKind::EC2, 61.0, Some(47.20), Some("0800-2000 daily"))]);
        assert!(output.contains("i-0abc123"));
    }

    #[test]
    fn test_render_shows_cta() {
        let output = render(&[]);
        assert!(output.contains("uptimescheduler.com"));
    }

    #[test]
    fn test_render_na_when_no_cost() {
        let output = render(&[make_analysis("i-xyz", ResourceKind::EC2, 50.0, None, None)]);
        assert!(output.contains("n/a"));
    }

    #[test]
    fn test_render_shows_schedule() {
        let output = render(&[make_analysis("i-abc", ResourceKind::EC2, 60.0, None, Some("0800-2000 daily"))]);
        assert!(output.contains("0800-2000"));
    }
}
