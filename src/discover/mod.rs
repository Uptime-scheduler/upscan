pub mod ec2;
pub mod ecs;
pub mod nat;
pub mod rds;

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    EC2,
    ECS,
    RDS,
    NatGateway,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub id: String,
    pub name: Option<String>,
    pub kind: ResourceKind,
    pub instance_type: Option<String>,
    pub region: String,
    pub tags: HashMap<String, String>,
    pub hourly_on_demand_cost: Option<f64>,
}
