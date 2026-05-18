use std::fs;

use anyhow::{Context, Result};
use clap::Parser;

use crate::configuration::{
  args::{Args, ProviderType},
  configuration::{
    AppConfiguration, DefaultProvider, ProviderConfiguration, ProviderDetails,
  },
};

pub fn init_config() -> Result<AppConfiguration>
{
  rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .expect("Failed to install rustls crypto provider");

  let args = Args::parse();

  let mut config = if fs::metadata(&args.config).is_ok() {
    let content = fs::read_to_string(&args.config).with_context(|| {
      format!("Failed to read config file: {}", args.config)
    })?;
    toml::from_str::<AppConfiguration>(&content)
      .with_context(|| format!("Failed to parse TOML from: {}", args.config))?
  } else {
    if matches!(args.provider_type, Some(ProviderType::FromConfigFile)) {
      anyhow::bail!(
        "Subcommand 'from-config-file', but file '{}' not founded.",
        args.config
      );
    }
    AppConfiguration::default()
  };

  if let Some(listening) = args.listening_on {
    config.common.listening_on = listening;
  }
  if let Some(peer) = args.peer_addr {
    config.common.peer_addr = peer;
  }

  if let Some(provider_type) = args.provider_type
    && !matches!(provider_type, ProviderType::FromConfigFile)
  {
    let common = args.provider_common.unwrap_or_default();

    let details = match provider_type {
      ProviderType::Direct => ProviderDetails::Direct,
      ProviderType::Default { kind, link } => {
        ProviderDetails::Default { kind, link }
      }
      ProviderType::Custom {
        username,
        password,
        turn_address,
        stun_address,
        realm,
      } => ProviderDetails::Custom {
        username,
        password,
        turn_address,
        stun_address,
        realm,
      },
      _ => unreachable!(),
    };

    config.providers = vec![ProviderConfiguration {
      priority: Some(0),
      using_udp: common.using_udp,
      using_dtls_obfuscation: common.using_dtls_obfuscation,
      details,
      threads: common.threads.map(|t| t as usize),
    }];
  }

  for provider in &mut config.providers {
    apply_provider_defaults(provider);
  }

  Ok(config)
}

fn apply_provider_defaults(provider: &mut ProviderConfiguration)
{
  if provider.threads.is_none() {
    provider.threads = match &provider.details {
      ProviderDetails::Default {
        kind: DefaultProvider::VkCalls,
        ..
      } => Some(16),
      ProviderDetails::Direct => None,
      _ => Some(1),
    };
  }
}
