pub mod cloudwatch;

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Datapoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
}

/// Metrics fetched for one resource.
/// `primary` is the main hourly series used for idle detection and the HTML heatmap.
/// `named` holds additional metric series keyed by metric name — used for composite
/// idle signals (e.g. RDS ReadIOPS + WriteIOPS, EC2 NetworkPacketsIn).
#[derive(Debug, Clone)]
pub struct ResourceMetrics {
    pub primary: Vec<Datapoint>,
    pub named: HashMap<String, Vec<Datapoint>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_datapoint_clone() {
        let dp = Datapoint { timestamp: Utc::now(), value: 42.5 };
        let dp2 = dp.clone();
        assert_eq!(dp2.value, 42.5);
    }
}
