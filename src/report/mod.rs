pub mod csv;
pub mod html;
pub mod json;
pub mod table;

/// Parse "HHMM-HHMM day-pattern" and return estimated monthly off-hours.
/// Uses a 30-day month with a 5/7 weekday and 2/7 weekend split.
pub fn schedule_monthly_off_hours(sched: &str) -> Option<f64> {
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
