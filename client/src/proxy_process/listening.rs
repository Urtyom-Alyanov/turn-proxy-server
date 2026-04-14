use std::{
  net::{IpAddr, SocketAddr},
  sync::Arc,
  time::Duration,
};

use anyhow::{Context, Result};
use dtls::config::Config as DtlsConfig;
use tokio::net::UdpSocket;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use turn_proxy_lib::balancer::{BalancedConn, ConnFactory};

use crate::{
  configuration::configuration::AppConfiguration,
  inbound::interface::get_current_interface,
  proxy_process::{
    run_bridge_group::run_bridge_thread, setup_connection::setup_connection,
  },
};

pub async fn listening(
  config: AppConfiguration,
  dtls_config: DtlsConfig,
) -> Result<()>
{
  let listen_addr: SocketAddr = config
    .common
    .listening_on
    .parse()
    .context("'listening-on' is not a valid socket address")?;
  let peer_addr: SocketAddr = config
    .common
    .peer_addr
    .parse()
    .context("'proxy-into' is not a valid socket address")?;

  info!("Listening on: {} UDP", listen_addr);
  info!("Proxying to: {} DTLS UDP", peer_addr);

  let listen_socket: Arc<UdpSocket> =
    Arc::new(UdpSocket::bind(listen_addr).await?);

  let cancel_token = CancellationToken::new();

  let ct = cancel_token.clone();
  tokio::spawn(async move {
    tokio::signal::ctrl_c().await.ok();
    info!("Shutdown signal received. Closing connections...");
    ct.cancel();
  });

  info!("Sorting providers with priorities...");
  let mut providers = config.providers.clone();
  providers.sort_by_key(|p| p.priority.unwrap_or(u32::MAX));

  loop {
    if cancel_token.is_cancelled() {
      break;
    }

    for provider in &providers {
      info!(
        "Trying provider with priority {:?}",
        provider.priority.unwrap_or(1)
      );

      let thread_count = provider.threads.unwrap_or(1);

      let interface_addr = match config.common.interface_addr.as_ref() {
        Some(s) => s
          .parse::<IpAddr>()
          .unwrap_or(get_current_interface().await?),
        None => get_current_interface().await?,
      };

      let p_clone = provider.clone();
      let dtls_cfg = dtls_config.clone();
      let factory: ConnFactory = Arc::new(move || {
        let p_inner = p_clone.clone();
        let cfg_inner = dtls_cfg.clone();
        Box::pin(async move {
          setup_connection(
            "BalancedWorker",
            interface_addr,
            &p_inner,
            peer_addr,
            cfg_inner,
          )
          .await
          .map_err(|e| webrtc_util::Error::Other(e.to_string()))
        })
      });

      let balanced_res =
        BalancedConn::new(thread_count, factory, cancel_token.child_token())
          .await;

      let balanced_conn = match balanced_res {
        Ok(c) => c,
        Err(e) => {
          error!("Failed to initialize balancer for provider: {:?}", e);
          continue;
        }
      };

      let bridge_res = run_bridge_thread(
        0,
        listen_socket.clone(),
        balanced_conn,
        cancel_token.child_token(),
      )
      .await;

      match bridge_res {
        Ok(_) => warn!("Bridge finished successfully. Switching provider..."),
        Err(e) => error!("Bridge error: {}. Switching provider...", e),
      }

      if cancel_token.is_cancelled() {
        break;
      }
    }

    if !cancel_token.is_cancelled() {
      warn!("All providers failed or finished. Retrying in 5s...");
      tokio::select! {
        _ = cancel_token.cancelled() => break,
        _ = tokio::time::sleep(Duration::from_secs(5)) => {}
      }
    }
  }

  info!("Terminating...");
  // let _ = tokio::time::timeout(Duration::from_secs(3), async {
  //   while let Some(_) = cancel_set.join_next().await {}
  // }).await;

  Ok(())
}
