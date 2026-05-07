mod analysis;
mod auth;
mod cli;
mod discover;
mod metrics;
mod pricing;
mod report;

use std::io::Write;
use std::sync::Arc;

use anyhow::Context;
use chrono::Utc;
use clap::Parser;
use futures::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};

use analysis::idle::{analyse_resource, AnalysisConfig};
use cli::{Args, OutputFormat, parse_resource_kinds, parse_tag_filter};
use discover::ResourceKind;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let config = Arc::new(
        auth::build_config(&args)
            .await
            .context("Failed to load AWS credentials")?,
    );

    let kinds = parse_resource_kinds(args.resources.as_deref())?;

    let tag_kv: Option<(String, String)> = args
        .tag_filter
        .as_deref()
        .map(parse_tag_filter)
        .transpose()?;
    let tag_ref: Option<(&str, &str)> =
        tag_kv.as_ref().map(|(k, v)| (k.as_str(), v.as_str()));

    if args.days < 7 {
        eprintln!(
            "Warning: --days {} is too short for reliable analysis. \
             Pattern detection and saving estimates need at least 7 days; 30 is recommended.",
            args.days
        );
    }

    eprintln!("Discovering resources in {}…", args.region);
    let mut resources = Vec::new();

    if kinds.contains(&ResourceKind::EC2) {
        match discover::ec2::discover(&config, &args.region, tag_ref).await {
            Ok(mut r) => { eprintln!("  EC2: {} instance(s)", r.len()); resources.append(&mut r); }
            Err(e) => eprintln!("Warning: EC2 discovery failed: {e:#}"),
        }
    }
    if kinds.contains(&ResourceKind::ECS) {
        match discover::ecs::discover(&config, &args.region, tag_ref).await {
            Ok(mut r) => { eprintln!("  ECS: {} service(s)", r.len()); resources.append(&mut r); }
            Err(e) => eprintln!("Warning: ECS discovery failed: {e:#}"),
        }
    }
    if kinds.contains(&ResourceKind::RDS) {
        match discover::rds::discover(&config, &args.region, tag_ref).await {
            Ok(mut r) => { eprintln!("  RDS: {} instance(s)", r.len()); resources.append(&mut r); }
            Err(e) => eprintln!("Warning: RDS discovery failed: {e:#}"),
        }
    }
    if kinds.contains(&ResourceKind::NatGateway) {
        match discover::nat::discover(&config, &args.region, tag_ref).await {
            Ok(mut r) => { eprintln!("  NAT: {} gateway(s)", r.len()); resources.append(&mut r); }
            Err(e) => eprintln!("Warning: NAT discovery failed: {e:#}"),
        }
    }

    if resources.is_empty() {
        eprintln!("No resources found. Check your region, tag filter, and permissions.");
        return Ok(());
    }

    eprintln!("Fetching Cost Explorer data…");
    // Keep a fallback copy so we can continue if pricing enrichment fails.
    let fallback = resources.clone();
    let mut resources = match pricing::enrich(&config, resources, args.days).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Warning: pricing enrichment failed: {e}");
            fallback
        }
    };
    // CE cannot price NAT gateways (no instance-type dimension). Apply hardcoded
    // regional rates for any NAT resource that still has no cost after CE enrichment.
    pricing::apply_fallback_pricing(&mut resources);

    let lookback_start = Utc::now() - chrono::Duration::days(args.days as i64);
    let lookback_end = Utc::now();

    let pb = ProgressBar::new(resources.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} fetching metrics…")
            .unwrap()
            .progress_chars("=>-"),
    );

    let futs = resources.iter().map(|r| {
        let cfg = config.clone();
        let res = r.clone();
        let pb = pb.clone();
        async move {
            let result = metrics::cloudwatch::fetch(&cfg, &res, lookback_start, lookback_end).await;
            pb.inc(1);
            (res, result)
        }
    });

    let metric_results: Vec<_> = join_all(futs).await;
    pb.finish_and_clear();

    let lookback_hours = args.days as f64 * 24.0;
    let analysis_config = AnalysisConfig {
        cpu_threshold:       args.threshold,
        min_idle_hours:      args.min_idle_hours,
        nat_bytes_threshold: args.nat_bytes_threshold,
    };

    let mut analyses = Vec::new();
    let mut skipped: Vec<String> = Vec::new();

    for (resource, result) in metric_results {
        match result {
            Ok(m) => analyses.push(analyse_resource(
                resource, m, &analysis_config, lookback_hours, lookback_start,
            )),
            Err(e) => {
                eprintln!("Warning: metrics fetch failed for {}: {e:#}", resource.id);
                skipped.push(resource.id);
            }
        }
    }

    analyses.sort_by(|a, b| {
        b.estimated_monthly_saving
            .unwrap_or(0.0)
            .partial_cmp(&a.estimated_monthly_saving.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let output = match args.output {
        OutputFormat::Table => report::table::render(&analyses),
        OutputFormat::Json  => report::json::render(&analyses)?,
        OutputFormat::Csv   => report::csv::render(&analyses),
        OutputFormat::Html  => report::html::render(&analyses, &args.region, args.days),
    };

    if let Some(path) = &args.save {
        std::fs::write(path, &output)
            .with_context(|| format!("Failed to write to {}", path.display()))?;
        eprintln!("Report written to {}", path.display());
    } else {
        print!("{output}");
        std::io::stdout().flush().ok();
    }

    if !skipped.is_empty() {
        eprintln!("\nSkipped {} resource(s) due to metrics fetch errors:", skipped.len());
        for id in &skipped { eprintln!("  - {id}"); }
    }

    Ok(())
}
