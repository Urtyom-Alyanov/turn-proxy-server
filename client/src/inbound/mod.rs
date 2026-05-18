use std::{net::IpAddr, sync::Arc};

use anyhow::Result;
use reqwest::Client;
use tracing::info;

use crate::inbound::{client::create_client, dns::configure_yandex_dns};

mod client;
pub mod dns;
pub mod interface;
pub mod user_agent;

/// Создаём исходный клиент для запросов к провайдерам внутри белых списков
pub async fn create_inbound_client(ip_interface: IpAddr) -> Result<Client>
{
  info!("Creating inbound client...");
  let dns = configure_yandex_dns()?;
  info!("Yandex DNS configured successfully");

  let user_agent = user_agent::get_random_user_agent();
  info!("Selected random user agent: {}", user_agent.value);

  let client = create_client(ip_interface, Arc::new(dns), &user_agent)?;
  info!(
    "Inbound client created successfully with IP interface: {}",
    ip_interface
  );

  Ok(client)
}
