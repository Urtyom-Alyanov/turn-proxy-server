pub mod configuration;
pub mod dtls;
pub mod inbound;
pub mod logging;
pub mod providers;
pub mod proxy_process;

use anyhow::Result;

use crate::{
  configuration::init_configuration::init_config,
  dtls::dtls_configure::dtls_configure, logging::init_logger,
  proxy_process::listening::listening,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()>
{
  let _guard = init_logger();
  let dns_provider = inbound::dns::configure_yandex_dns()?;
  let config = init_config()?;
  let dtls_config = dtls_configure()?;

  listening(config, dtls_config, dns_provider).await?;

  Ok(())
}
