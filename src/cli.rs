use std::collections::HashSet;
use std::path::PathBuf;
use clap::Parser;
use crate::discover::ResourceKind;

#[derive(Parser, Debug)]
#[command(
    name = "upscan",
    about = "Analyse AWS resource usage and generate idle-period reports",
    long_about = None
)]
pub struct Args {
    /// AWS profile name (falls back to env/SSO/instance role)
    #[arg(long)]
    pub profile: Option<String>,

    /// AWS region
    #[arg(long, default_value = "us-east-1")]
    pub region: String,

    /// Lookback window in days
    #[arg(long, default_value = "30")]
    pub days: u32,

    /// CPU idle threshold % for EC2/ECS
    #[arg(long, default_value = "5.0")]
    pub threshold: f64,

    /// NAT: hourly bytes-out below which an hour is idle (default 1 KB/hr)
    #[arg(long, default_value = "1000.0")]
    pub nat_bytes_threshold: f64,

    /// Comma-separated resource types: ec2,ecs,rds,nat (default: all)
    #[arg(long)]
    pub resources: Option<String>,

    /// Output format
    #[arg(long, default_value = "table", value_enum)]
    pub output: OutputFormat,

    /// Only include resources matching this tag, e.g. Environment=dev
    #[arg(long)]
    pub tag_filter: Option<String>,

    /// Write report output to this file path
    #[arg(long)]
    pub save: Option<PathBuf>,

    /// Minimum contiguous idle block to report (hours)
    #[arg(long, default_value = "2.0")]
    pub min_idle_hours: f64,

}

#[derive(Debug, Clone, PartialEq, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
    Html,
}

/// Parse "--resources ec2,rds,nat" into a HashSet<ResourceKind>.
/// Returns all kinds when input is None.
pub fn parse_resource_kinds(input: Option<&str>) -> anyhow::Result<HashSet<ResourceKind>> {
    match input {
        None => Ok([
            ResourceKind::EC2,
            ResourceKind::ECS,
            ResourceKind::RDS,
            ResourceKind::NatGateway,
        ]
        .into_iter()
        .collect()),
        Some(s) => s
            .split(',')
            .map(|token| match token.trim().to_lowercase().as_str() {
                "ec2" => Ok(ResourceKind::EC2),
                "ecs" => Ok(ResourceKind::ECS),
                "rds" => Ok(ResourceKind::RDS),
                "nat" => Ok(ResourceKind::NatGateway),
                other => anyhow::bail!("Unknown resource type: {other}. Valid: ec2, ecs, rds, nat"),
            })
            .collect(),
    }
}

/// Parse "Key=Value" tag filter into (key, value).
pub fn parse_tag_filter(input: &str) -> anyhow::Result<(String, String)> {
    let (k, v) = input
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("--tag-filter must be in Key=Value format, got: {input}"))?;
    Ok((k.to_string(), v.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_defaults() {
        let args = Args::try_parse_from(["upscan"]).unwrap();
        assert_eq!(args.region, "us-east-1");
        assert_eq!(args.days, 30);
        assert_eq!(args.threshold, 5.0);
        assert_eq!(args.min_idle_hours, 2.0);
        assert_eq!(args.nat_bytes_threshold, 1000.0);
        assert_eq!(args.output, OutputFormat::Table);
        assert!(args.profile.is_none());
        assert!(args.resources.is_none());
        assert!(args.tag_filter.is_none());
        assert!(args.save.is_none());
    }

    #[test]
    fn test_parse_resource_kinds_all() {
        let kinds = parse_resource_kinds(None).unwrap();
        assert!(kinds.contains(&ResourceKind::EC2));
        assert!(kinds.contains(&ResourceKind::ECS));
        assert!(kinds.contains(&ResourceKind::RDS));
        assert!(kinds.contains(&ResourceKind::NatGateway));
    }

    #[test]
    fn test_parse_resource_kinds_subset() {
        let kinds = parse_resource_kinds(Some("ec2,rds")).unwrap();
        assert!(kinds.contains(&ResourceKind::EC2));
        assert!(kinds.contains(&ResourceKind::RDS));
        assert!(!kinds.contains(&ResourceKind::ECS));
        assert!(!kinds.contains(&ResourceKind::NatGateway));
    }

    #[test]
    fn test_parse_tag_filter() {
        let (k, v) = parse_tag_filter("Environment=dev").unwrap();
        assert_eq!(k, "Environment");
        assert_eq!(v, "dev");
    }

    #[test]
    fn test_parse_tag_filter_invalid() {
        assert!(parse_tag_filter("NoEquals").is_err());
    }
}
