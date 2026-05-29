use crate::analysis::idle::ResourceAnalysis;
use crate::discover::ResourceKind;

pub fn render(analyses: &[ResourceAnalysis]) -> String {
    let headers = [
        "Resource",
        "Type",
        "Idle %",
        "Est. Cost/mo",
        "Saving/mo",
        "Suggested Schedule",
    ];

    let mut rows: Vec<[String; 6]> = Vec::new();
    let mut total_saving = 0.0f64;
    let mut has_any_cost = false;

    for a in analyses {
        let resource_label = match &a.resource.name {
            Some(name) => name.clone(),
            None => a.resource.id.clone(),
        };

        let type_str = match a.resource.kind {
            ResourceKind::EC2 => match &a.resource.instance_type {
                Some(t) => format!("EC2 {t}"),
                None => "EC2".to_string(),
            },
            ResourceKind::ECS => "ECS Fargate".to_string(),
            ResourceKind::RDS => match &a.resource.instance_type {
                Some(t) => format!("RDS {t}"),
                None => "RDS".to_string(),
            },
            ResourceKind::NatGateway => "NAT GW".to_string(),
        };

        let idle_pct = format!("{:.0} %", a.idle_percentage.abs());

        let cost_str = match a.resource.hourly_on_demand_cost {
            Some(c) => format!("${:.2}", c * 720.0),
            None => "n/a".to_string(),
        };

        let saving_str = match (a.suggested_schedule.as_deref(), a.resource.hourly_on_demand_cost) {
            (Some(sched), Some(cost)) => {
                if let Some(off_hours) = schedule_monthly_off_hours(sched) {
                    let saving = off_hours * cost;
                    total_saving += saving;
                    has_any_cost = true;
                    format!("${:.2}", saving)
                } else {
                    "n/a".to_string()
                }
            }
            _ => "n/a".to_string(),
        };

        let schedule = format_schedule(a.suggested_schedule.as_deref());

        rows.push([resource_label, type_str, idle_pct, cost_str, saving_str, schedule]);
    }

    // Calculate column widths
    let mut widths = [0usize; 6];
    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.len();
    }
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.len());
        }
    }

    let col_gap = "   ";
    let mut out = String::new();

    // Header row
    out.push(' ');
    for (i, h) in headers.iter().enumerate() {
        out.push_str(&format!("{:<width$}", h, width = widths[i]));
        if i < headers.len() - 1 {
            out.push_str(col_gap);
        }
    }
    out.push('\n');

    // Separator
    let total_width = widths.iter().sum::<usize>() + (headers.len() - 1) * col_gap.len() + 1;
    out.push_str(&"─".repeat(total_width));
    out.push('\n');

    // Data rows
    for row in &rows {
        out.push(' ');
        for (i, cell) in row.iter().enumerate() {
            out.push_str(&format!("{:<width$}", cell, width = widths[i]));
            if i < row.len() - 1 {
                out.push_str(col_gap);
            }
        }
        out.push('\n');
    }

    out.push('\n');

    if has_any_cost && total_saving > 0.0 {
        out.push_str(&format!("Total potential saving: ${:.2}/month\n", total_saving));
    }
    out.push_str("\n→ Reduce this bill with Uptime Scheduler: https://uptimescheduler.com\n");
    out
}

/// Parse "HHMM-HHMM day-pattern" and return estimated monthly off-hours.
/// Uses a 30-day month with a 5/7 weekday and 2/7 weekend split.
fn schedule_monthly_off_hours(sched: &str) -> Option<f64> {
    let parts: Vec<&str> = sched.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let times: Vec<&str> = parts[0].splitn(2, '-').collect();
    if times.len() != 2 {
        return None;
    }
    let parse_hour = |t: &str| -> Option<f64> {
        if t.len() == 4 {
            let h: f64 = t[..2].parse().ok()?;
            let m: f64 = t[2..].parse().ok()?;
            Some(h + m / 60.0)
        } else {
            None
        }
    };
    let start = parse_hour(times[0])?;
    let end = parse_hour(times[1])?;
    if end <= start {
        return None;
    }
    let off_per_day = 24.0 - (end - start);
    const MONTH: f64 = 30.0;
    let wd = MONTH * 5.0 / 7.0;
    let we = MONTH * 2.0 / 7.0;
    let off_hours = match parts[1] {
        "mon-fri" => off_per_day * wd + 24.0 * we,
        "sat-sun" => off_per_day * we + 24.0 * wd,
        "daily"   => off_per_day * MONTH,
        _         => return None,
    };
    Some(off_hours)
}

/// Convert internal schedule format ("0800-1800 mon-fri") to a human-readable form
/// ("Mon–Fri 08:00–18:00").
fn format_schedule(sched: Option<&str>) -> String {
    let s = match sched {
        Some(s) => s,
        None => return "—".to_string(),
    };

    let parts: Vec<&str> = s.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return s.to_string();
    }

    let times: Vec<&str> = parts[0].splitn(2, '-').collect();
    if times.len() != 2 {
        return s.to_string();
    }

    let fmt_time = |t: &str| -> String {
        if t.len() == 4 {
            format!("{}:{}", &t[..2], &t[2..])
        } else {
            t.to_string()
        }
    };

    let days = match parts[1] {
        "mon-fri" => "Mon–Fri",
        "sat-sun" => "Sat–Sun",
        "daily"   => "Daily",
        other     => other,
    };

    format!("{} {}–{}", days, fmt_time(times[0]), fmt_time(times[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::idle::ResourceAnalysis;
    use crate::discover::{Resource, ResourceKind};
    use std::collections::HashMap;

    fn make_analysis(
        id: &str,
        kind: ResourceKind,
        idle_pct: f64,
        saving: Option<f64>,
        schedule: Option<&str>,
    ) -> ResourceAnalysis {
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
        let output = render(&[make_analysis(
            "i-0abc123",
            ResourceKind::EC2,
            61.0,
            Some(47.20),
            Some("0800-2000 daily"),
        )]);
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
        let output = render(&[make_analysis(
            "i-abc",
            ResourceKind::EC2,
            60.0,
            None,
            Some("0800-2000 daily"),
        )]);
        assert!(output.contains("08:00"));
    }

    #[test]
    fn test_format_schedule_mon_fri() {
        assert_eq!(format_schedule(Some("0900-1700 mon-fri")), "Mon–Fri 09:00–17:00");
    }

    #[test]
    fn test_format_schedule_daily() {
        assert_eq!(format_schedule(Some("0700-2100 daily")), "Daily 07:00–21:00");
    }

    #[test]
    fn test_format_schedule_none() {
        assert_eq!(format_schedule(None), "—");
    }

    #[test]
    fn test_type_column_includes_instance_type() {
        let output = render(&[make_analysis(
            "i-abc",
            ResourceKind::EC2,
            60.0,
            None,
            None,
        )]);
        assert!(output.contains("EC2 t3.large"));
    }

    #[test]
    fn test_nat_gw_type_label() {
        let output = render(&[make_analysis("nat-xyz", ResourceKind::NatGateway, 80.0, Some(30.0), None)]);
        assert!(output.contains("NAT GW"));
    }
}
