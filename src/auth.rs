use aws_config::SdkConfig;
use crate::cli::Args;

/// Build an AWS SdkConfig from CLI args.
/// Uses --profile if provided, otherwise falls back to the default
/// credential chain (env vars → SSO → instance role).
pub async fn build_config(args: &Args) -> anyhow::Result<SdkConfig> {
    let region = aws_config::meta::region::RegionProviderChain::first_try(
        aws_sdk_ec2::config::Region::new(args.region.clone()),
    );

    let loader = aws_config::defaults(aws_config::BehaviorVersion::latest()).region(region);

    let loader = if let Some(profile) = &args.profile {
        loader.profile_name(profile)
    } else {
        loader
    };

    let config = loader.load().await;

    Ok(config)
}
