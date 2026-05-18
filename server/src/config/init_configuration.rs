use std::fs;

use anyhow::{Context, Result};
use clap::Parser;

use crate::config::{
  args::Args,
  configuration::{AppConfig, CommonConfig},
};

pub fn init_config() -> Result<AppConfig>
{
  rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .expect("Failed to install rustls crypto provider");

  let args = Args::parse();

  let config = if !args.no_config {
    let content = fs::read_to_string(&args.config)
      .context(format!("read configuration file error: {}", args.config))?;
    toml::from_str::<AppConfig>(&content).context(format!(
      "TOML configuration parse error (path: {})",
      args.config
    ))?
  } else {
    AppConfig::default()
  };

  let final_listen = args
    .listening_on
    .or(config.common.listening_on)
    .unwrap_or_else(|| "0.0.0.0:56000".to_string());

  let final_proxy = args
    .proxy_into
    .or(config.common.proxy_into)
    .context("`proxy_into` address is missing")?;

  let max_connections = args
    .max_connections
    .or(config.common.max_connections)
    .context("`max_connections` is missing")?;

  Ok(AppConfig {
    common: CommonConfig {
      listening_on: final_listen.into(),
      proxy_into: final_proxy.into(),
      max_connections: max_connections.into(),
    },
  })
}
