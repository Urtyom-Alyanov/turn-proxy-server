use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{Result, anyhow};
use bytes::BytesMut;
use tokio::{sync::RwLock, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use webrtc_util::Conn;

/// Низкоуровневая абстракция
pub fn proxy_flow(
  flow_name: String,
  cancellation_token: CancellationToken,

  _from_addr: SocketAddr,
  to_addr: SocketAddr,

  from_flow: Arc<dyn Conn + Send + Sync>,
  to_flow: Arc<dyn Conn + Send + Sync>,

  from_cache: Option<Arc<RwLock<Option<SocketAddr>>>>,
  to_cache: Option<Arc<RwLock<Option<SocketAddr>>>>,

  idle_timeout: Option<Duration>,
) -> JoinHandle<Result<()>>
{
  tokio::spawn(async move {
    let mut buf = BytesMut::with_capacity(4096);

    loop {
      let recv_future = from_flow.recv_from(&mut buf);

      let res = tokio::select! {
        // Отмена потока
        _ = cancellation_token.cancelled() => break,

        // Получение данных с возможным таймаутом бездействия
        recv_result = async {
          if let Some(t) = idle_timeout {
            tokio::time::timeout(t, recv_future).await.map_err(|_| anyhow!("Idle timeout reached"))
          } else {
            Ok(recv_future.await)
          }
        } => {
          match recv_result {
            Ok(Ok(res)) => res,
            Ok(Err(e)) => {
              warn!("[{}] Receive error: {}", flow_name, e);
              break;
            }
            Err(_) => {
              debug!("[{}] Idle timeout reached", flow_name);
              break;
            }
          }
        }
      };

      let (n, src) = res;

      if n == 0 {
        break;
      }

      if let Some(cache) = &from_cache {
        *cache.write().await = Some(src);
      }

      debug!("[{}] Received {} bytes from {}", flow_name, n, src);

      if n >= buf.capacity() {
        warn!(
          "[{}] Packet from {} is too large for allocator (size: {} bytes)",
          flow_name, src, n
        );
      }

      let data = buf.split_to(n).freeze();

      let (send, dest) = if let Some(cache) = &to_cache {
        let dest = cache.read().await.unwrap_or(to_addr);
        (to_flow.send_to(&data, dest), dest)
      } else {
        (to_flow.send(&data), to_addr)
      };

      let sent_bytes = match send.await {
        Ok(n) => n,
        Err(e) => {
          warn!("[{}] Send error to {}: {}", flow_name, dest, e);
          break;
        }
      };

      debug!("[{}] Sent {} bytes to {}", flow_name, sent_bytes, dest);
    }

    cancellation_token.cancel();
    info!("[{}] Flow stopped", flow_name);
    Ok(())
  })
}
