pub mod config;
pub mod dtls_process;
pub mod logging;
pub mod proxy_process;

use anyhow::Result;

use crate::{
  config::init_configuration::init_config, dtls_process::dtls_configure,
  logging::init_logging, proxy_process::listening::listening,
};

#[tokio::main]
async fn main() -> Result<()>
{
  let _guard = init_logging();
  let dns_provider = config::dns::configure_system_dns()?;
  let config = init_config()?;
  let dtls_config = dtls_configure()?;

  listening(config, dtls_config, dns_provider).await?;

  Ok(())
}
