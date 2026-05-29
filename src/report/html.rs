use chrono::{Duration, Timelike, Utc};
use crate::analysis::idle::ResourceAnalysis;
use crate::discover::ResourceKind;
use crate::metrics::Datapoint;
use super::schedule_monthly_off_hours;

/// Colour a heatmap cell relative to the resource's idle threshold.
/// ratio < 0.3  → red   (clearly idle)
/// ratio < 0.75 → orange (approaching idle boundary)
/// ratio < 1.5  → yellow (near/just above threshold)
/// ratio ≥ 1.5  → green  (clearly active)
fn util_color(val: Option<f64>, threshold: f64) -> &'static str {
    match val {
        None => "#cccccc",
        Some(v) => {
            let ratio = if threshold > 0.0 { v / threshold } else { v };
            if ratio < 0.3        { "#e74c3c" }
            else if ratio < 0.75  { "#f39c12" }
            else if ratio < 1.5   { "#f1c40f" }
            else                  { "#2ecc71" }
        }
    }
}

fn build_heatmap_grid(series: &[Datapoint]) -> Vec<Vec<Option<f64>>> {
    let now = Utc::now();
    let today = now.date_naive();
    let cutoff = now - Duration::days(7);
    // Only show completed hours — skip the current partial hour.
    let current_hour = now
        .with_minute(0).unwrap()
        .with_second(0).unwrap()
        .with_nanosecond(0).unwrap();
    let mut grid: Vec<Vec<Vec<f64>>> = vec![vec![vec![]; 24]; 7];
    for dp in series {
        if dp.timestamp < cutoff { continue; }
        if dp.timestamp >= current_hour { continue; } // skip incomplete/future hours
        // Use calendar-day bucketing so yesterday's 23:00 doesn't appear in today's row.
        let dp_date = dp.timestamp.date_naive();
        let days_ago = (today - dp_date).num_days();
        if days_ago < 0 || days_ago >= 7 { continue; }
        let day_idx = 6 - days_ago as usize;
        let hour = dp.timestamp.hour() as usize;
        if hour < 24 { grid[day_idx][hour].push(dp.value); }
    }
    grid.iter().map(|day| {
        day.iter().map(|vals| {
            if vals.is_empty() { None }
            else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
        }).collect()
    }).collect()
}

fn render_heatmap(series: &[Datapoint], threshold: f64) -> String {
    let grid = build_heatmap_grid(series);
    let today = Utc::now().date_naive();
    let mut html = String::from(
        "<div class=\"heatmap-wrap\"><div class=\"heatmap-hours\"><div class=\"heatmap-day-label\"></div>",
    );
    for h in 0..24 { html.push_str(&format!("<div class=\"heatmap-hour-label\">{h:02}</div>")); }
    html.push_str("</div>\n");
    for (day_idx, day_vals) in grid.iter().enumerate() {
        let days_ago = 6 - day_idx;
        let day_dt = today - chrono::naive::Days::new(days_ago as u64);
        let day_label = day_dt.format("%a %m/%d").to_string();
        html.push_str(&format!(
            "<div class=\"heatmap-row\"><div class=\"heatmap-day-label\">{day_label}</div>"
        ));
        for (hour, val) in day_vals.iter().enumerate() {
            let color = util_color(*val, threshold);
            let title = match val {
                None    => format!("{day_label} {hour:02}:00 — no data"),
                Some(v) => format!("{day_label} {hour:02}:00 — {v:.1}"),
            };
            html.push_str(&format!(
                "<div class=\"heatmap-cell\" style=\"background:{color}\" title=\"{title}\"></div>"
            ));
        }
        html.push_str("</div>\n");
    }
    html.push_str("</div>");
    html
}

fn schedule_saving(a: &ResourceAnalysis) -> Option<f64> {
    let sched = a.suggested_schedule.as_deref()?;
    let cost = a.resource.hourly_on_demand_cost?;
    Some(schedule_monthly_off_hours(sched)? * cost)
}

pub fn render(analyses: &[ResourceAnalysis], region: &str, days: u32) -> String {
    let now = Utc::now();
    let total_saving: f64 = analyses.iter().filter_map(schedule_saving).sum();
    let has_savings = total_saving > 0.0;

    let table_rows: String = analyses.iter().map(|a| {
        let kind_str = match a.resource.kind {
            ResourceKind::EC2 => "EC2", ResourceKind::ECS => "ECS",
            ResourceKind::RDS => "RDS", ResourceKind::NatGateway => "NAT",
        };
        let name = a.resource.name.as_deref().unwrap_or(a.resource.id.as_str());
        let itype = a.resource.instance_type.as_deref().unwrap_or("—");
        let pattern = a.primary_idle_pattern.as_deref().unwrap_or("—");
        let saving = schedule_saving(a).map(|s| format!("${s:.2}")).unwrap_or_else(|| "n/a".to_string());
        format!(
            "<tr><td>{name} <small>({itype})</small></td><td>{kind_str}</td><td>{pattern}</td><td>{:.0}%</td><td>{saving}</td></tr>",
            a.idle_percentage
        )
    }).collect::<Vec<_>>().join("\n");

    let total_row = if has_savings {
        format!("<tr class=\"total-row\"><td colspan=\"4\"><strong>Total potential saving</strong></td><td><strong>${total_saving:.2}/month</strong></td></tr>")
    } else { String::new() };

    let resources_html: String = analyses.iter().map(|a| {
        let name = a.resource.name.as_deref().unwrap_or(a.resource.id.as_str());
        let saving = schedule_saving(a).map(|s| format!("${s:.2}/mo")).unwrap_or_else(|| "n/a".to_string());
        let pattern = a.primary_idle_pattern.as_deref().unwrap_or("—");
        let schedule = a.suggested_schedule.as_deref().unwrap_or("—");
        let heatmap = render_heatmap(&a.utilisation_series, a.idle_threshold);
        let outside_block = if a.active_hours_outside_schedule > 0 {
            format!("<div class=\"outside-warn\">⚠ {} active hours fall outside the suggested schedule window — verify before scheduling</div>",
                a.active_hours_outside_schedule)
        } else { String::new() };
        format!(
            "<div class=\"resource-card\"><div class=\"resource-header\"><span class=\"resource-name\">{name}</span><span class=\"resource-saving\">{saving}</span></div><div class=\"resource-stats\"><div class=\"stat\"><span class=\"stat-label\">Idle</span><span class=\"stat-value\">{:.0}%</span></div><div class=\"stat\"><span class=\"stat-label\">Pattern</span><span class=\"stat-value\">{pattern}</span></div><div class=\"stat\"><span class=\"stat-label\">Schedule</span><span class=\"stat-value\">{schedule}</span></div></div>{outside_block}{heatmap}</div>",
            a.idle_percentage
        )
    }).collect::<Vec<_>>().join("\n");

    let saving_block = if has_savings {
        format!("<div class=\"saving-total\">${total_saving:.2}<span style=\"font-size:1.2rem;font-weight:400;color:#a0aec0\">/month potential saving</span></div>")
    } else { String::new() };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>upscan Report — {now_date}</title>
<style>
*{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#f4f6f8;color:#2d3748;padding:2rem;line-height:1.5}}
.header{{background:#1a202c;color:#fff;padding:2rem;border-radius:12px;margin-bottom:2rem}}
.header h1{{font-size:1.4rem;font-weight:600;margin-bottom:.5rem}}
.meta{{color:#a0aec0;font-size:.875rem}}
.saving-total{{font-size:2.5rem;font-weight:700;color:#48bb78;margin:.75rem 0}}
.section{{background:#fff;border-radius:12px;padding:1.5rem;margin-bottom:1.5rem;box-shadow:0 1px 3px rgba(0,0,0,.08)}}
.section h2{{font-size:1rem;font-weight:600;color:#718096;text-transform:uppercase;letter-spacing:.05em;margin-bottom:1rem}}
table{{width:100%;border-collapse:collapse}}
th{{text-align:left;padding:.625rem 1rem;border-bottom:2px solid #edf2f7;color:#718096;font-size:.8rem;text-transform:uppercase}}
td{{padding:.625rem 1rem;border-bottom:1px solid #edf2f7;font-size:.9rem}}
td small{{color:#a0aec0}}
.total-row td{{font-weight:600;border-top:2px solid #edf2f7;border-bottom:none}}
.resource-card{{background:#fff;border-radius:12px;padding:1.5rem;margin-bottom:1rem;box-shadow:0 1px 3px rgba(0,0,0,.08)}}
.resource-header{{display:flex;align-items:baseline;gap:1rem;margin-bottom:1rem;flex-wrap:wrap}}
.resource-name{{font-weight:600}}
.resource-saving{{margin-left:auto;font-weight:700;color:#48bb78}}
.resource-stats{{display:flex;gap:2rem;margin-bottom:1rem}}
.stat{{display:flex;flex-direction:column}}
.stat-label{{font-size:.75rem;color:#a0aec0;text-transform:uppercase}}
.stat-value{{font-weight:600}}
.warn-badge{{color:#c05621;font-size:.8rem;margin-left:.5rem}}
.outside-warn{{background:#fffaf0;border:1px solid #f6ad55;color:#c05621;padding:.625rem 1rem;border-radius:6px;font-size:.85rem;margin-bottom:1rem}}
.disclaimer{{background:#fffaf0;border:1px solid #f6ad55;color:#7b5213;padding:.75rem 1rem;border-radius:8px;font-size:.85rem;margin-bottom:1.25rem}}
.heatmap-wrap{{overflow-x:auto}}
.heatmap-hours,.heatmap-row{{display:grid;grid-template-columns:80px repeat(24,1fr);gap:2px;margin-bottom:2px}}
.heatmap-day-label{{font-size:.7rem;color:#a0aec0;display:flex;align-items:center;white-space:nowrap}}
.heatmap-hour-label{{font-size:.65rem;color:#a0aec0;text-align:center}}
.heatmap-cell{{height:18px;border-radius:2px}}
.legend{{display:flex;gap:1rem;flex-wrap:wrap;margin-top:.5rem}}
.legend-item{{display:flex;align-items:center;gap:.25rem;font-size:.75rem;color:#718096}}
.legend-swatch{{width:16px;height:16px;border-radius:2px}}
.cta{{background:linear-gradient(135deg,#667eea 0%,#764ba2 100%);color:#fff;padding:2rem;border-radius:12px;text-align:center;margin-top:2rem}}
.cta p{{margin-bottom:1rem;opacity:.9}}
.cta a{{display:inline-block;background:#fff;color:#667eea;padding:.75rem 2rem;border-radius:8px;text-decoration:none;font-weight:700}}
</style>
</head>
<body>
<div class="header">
  <h1>upscan Report</h1>
  <div class="meta">Region: {region} · Lookback: {days} days · Generated: {now_date}</div>
  {saving_block}
</div>
<div class="section">
  <h2>Summary</h2>
  <table>
    <thead><tr><th>Resource</th><th>Type</th><th>Idle pattern</th><th>Idle %</th><th>Est. saving/mo</th></tr></thead>
    <tbody>{table_rows}{total_row}</tbody>
  </table>
</div>
<div class="section">
  <h2>Resource Detail &amp; Utilisation Heatmaps</h2>
  <div class="disclaimer">&#9888; Suggested schedules are a guideline only. Review each resource carefully and verify the schedule against your actual workload before applying in production.</div>
  <div class="legend">
    <div class="legend-item"><div class="legend-swatch" style="background:#2ecc71"></div>Active</div>
    <div class="legend-item"><div class="legend-swatch" style="background:#f1c40f"></div>Near threshold</div>
    <div class="legend-item"><div class="legend-swatch" style="background:#f39c12"></div>Low</div>
    <div class="legend-item"><div class="legend-swatch" style="background:#e74c3c"></div>Idle</div>
    <div class="legend-item"><div class="legend-swatch" style="background:#cccccc"></div>No data</div>
  </div>
  {resources_html}
</div>
<div class="cta">
  <p>Schedule these resources automatically and start saving today.</p>
  <a href='https://uptimescheduler.com'>Get started with Uptime Scheduler →</a>
</div>
</body>
</html>"#,
        now_date = now.format("%Y-%m-%d %H:%M UTC"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::idle::ResourceAnalysis;
    use crate::discover::{Resource, ResourceKind};
    use std::collections::HashMap;
    use chrono::{TimeZone, Utc};

    fn make_analysis() -> ResourceAnalysis {
        let base = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let series: Vec<Datapoint> = (0..168u64).map(|h| Datapoint {
            timestamp: base + Duration::hours(h as i64),
            value: if h % 24 < 8 { 1.0 } else { 80.0 },
        }).collect();
        ResourceAnalysis {
            resource: Resource {
                id: "i-0abc123".to_string(), name: Some("web-server".to_string()),
                kind: ResourceKind::EC2, instance_type: Some("t3.large".to_string()),
                region: "us-east-1".to_string(), tags: HashMap::new(),
                hourly_on_demand_cost: Some(0.0832),
            },
            idle_windows: vec![], total_idle_hours: 70.0, idle_percentage: 41.7,
            estimated_monthly_saving: Some(47.20), primary_idle_pattern: Some("nights".to_string()),
            suggested_schedule: Some("0800-2000 daily".to_string()),
            idle_classification: None, utilisation_series: series, idle_threshold: 5.0,
            active_hours_outside_schedule: 0,
        }
    }

    #[test]
    fn test_html_is_valid_structure() {
        let out = render(&[make_analysis()], "us-east-1", 30);
        assert!(out.contains("<!DOCTYPE html>") && out.contains("</html>"));
    }

    #[test]
    fn test_html_contains_resource() {
        let out = render(&[make_analysis()], "us-east-1", 30);
        assert!(out.contains("web-server") || out.contains("i-0abc123"));
    }

    #[test]
    fn test_html_contains_cta() {
        let out = render(&[], "us-east-1", 30);
        assert!(out.contains("uptimescheduler.com"));
    }

    #[test]
    fn test_html_contains_heatmap_cells() {
        let out = render(&[make_analysis()], "us-east-1", 30);
        assert!(out.contains("heatmap-cell"));
    }

    #[test]
    fn test_html_no_external_resources() {
        let out = render(&[make_analysis()], "us-east-1", 30);
        assert!(!out.contains("href=\"http") && !out.contains("src=\"http"));
    }
}
