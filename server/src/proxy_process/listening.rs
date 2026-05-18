use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use dtls::{config::Config as DtlsConfig, listener::listen};
use reqwest::dns::{Name, Resolve};
use tokio::{sync::Semaphore, task::JoinSet};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use webrtc_util::conn::Listener;

use crate::{
  config::configuration::AppConfig,
  proxy_process::handle_encrypted_udp_connection::handle_encrypted_udp_connection,
};

pub async fn get_socket_addr(
  addr_str: &str,
  dns_provider: &impl Resolve,
) -> Result<SocketAddr>
{
  if let Ok(addr) = addr_str.parse::<SocketAddr>() {
    return Ok(addr);
  }

  info!(
    "IP is not present in peer address '{}', attempting DNS resolution...",
    addr_str
  );

  let (host, port_str) = addr_str.rsplit_once(':').context(
    "Listening address must include a port (e.g., 'wireguard:8080')",
  )?;

  let port: u16 = port_str
    .parse()
    .context("Port in listening address is not a valid number")?;

  let name = Name::from_str(host)
    .context("Failed to parse host in listening address")?;

  let ips = dns_provider.resolve(name).await;

  if let Err(e) = &ips {
    error!("DNS resolution failed for '{}': {}", host, e);
    return Err(anyhow::anyhow!(
      "DNS resolution failed for '{}': {}",
      host,
      e
    ));
  }

  let mut ips = ips.unwrap();

  let socket_addr = ips
    .next()
    .context("No IP addresses found for listening address")?;
  let ip = socket_addr.ip();

  info!("IP successfully resolved for '{}' ({})", addr_str, ip);

  Ok(SocketAddr::new(ip, port))
}

pub async fn listening(
  config: AppConfig,
  dtls_config: DtlsConfig,
  dns_provider: impl Resolve,
) -> Result<()>
{
  let listen_addr: SocketAddr = config
    .common
    .listening_on
    .unwrap()
    .parse()
    .context("'listening-on' is not a valid socket address")?;
  let proxy_addr = get_socket_addr(
    &config
      .common
      .proxy_into
      .expect("proxy-into has not provided"),
    &dns_provider,
  )
  .await
  .context("Failed to resolve proxy address")?;

  info!("Listening on: {} DTLS UDP", listen_addr);
  info!("Proxying to: {} UDP", proxy_addr);
  let listener = listen(listen_addr, dtls_config).await?;

  let cancel_token = CancellationToken::new();
  let mut cancel_set = JoinSet::new();

  let semaphore = Arc::new(Semaphore::new(
    config.common.max_connections.unwrap_or(2000),
  ));

  let ct = cancel_token.clone();
  tokio::spawn(async move {
    tokio::signal::ctrl_c().await.ok();
    info!("Shutdown signal received. Closing connections...");
    ct.cancel();
  });

  info!("Proxy server is up");

  loop {
    tokio::select! {
      _ = cancel_token.cancelled() => break,
      res = cancel_set.join_next(), if !cancel_set.is_empty() => {
        if let Some(Err(e)) = res {
          error!("Task panicked or failed: {:?}", e);
        }
      },
      conn_result = listener.accept() => {
        let (conn, remote_addr): (_, _) = match conn_result {
          Ok(res) => res,
          Err(e) => {
            if cancel_token.is_cancelled() { break; }
            warn!("Accept error: {}", e);
            continue;
          }
        };

        let semaphore_permit = semaphore.clone().try_acquire_owned();

        if let Ok(permit) = semaphore_permit {
          let ct_inner = cancel_token.clone();
          // let proxy_addr = proxy_addr;

          cancel_set.spawn(async move {
            info!("Connection from: {}", remote_addr);

            let _permit = permit;

            let conn_for_shutdown = conn.clone();

            tokio::select! {
              _ = ct_inner.cancelled() => {
                let _ = conn_for_shutdown.close().await;
              }
              res = handle_encrypted_udp_connection(conn, proxy_addr) => {
                if let Err(e) = res {
                  warn!("Error handling connection to {}: {}", remote_addr, e);
                }
              }
            }

            info!("Connection closed: {}", remote_addr);
          });
        } else {
          warn!("Max connections reached, dropping connection from {}", remote_addr);
          let _ = conn.close().await;
        }
      }
    }
  }

  info!("Waiting for all tasks to finish...");
  let _ = tokio::time::timeout(Duration::from_secs(3), async {
    while cancel_set.join_next().await.is_some() {}
  })
  .await;

  info!("Server stopped.");

  Ok(())
}
