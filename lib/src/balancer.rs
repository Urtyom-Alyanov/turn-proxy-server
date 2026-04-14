use std::{
  any::Any,
  net::SocketAddr,
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
  },
};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::future::{BoxFuture, join_all};
use parking_lot::RwLock;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use webrtc_util::{Conn, Error as WebRtcError, Result as WebRtcResult};

use crate::UDP_MTU;
pub const CHANNEL_BUF: usize = 1024;

const MIN_RETRY_DELAY_SECS: u64 = 5;
const MAX_RETRY_DELAY_SECS: u64 = 30;

const CONNECTION_CLOSING_DELAY: u64 = 5;

pub type ConnFactory = Arc<
  dyn Fn() -> BoxFuture<'static, WebRtcResult<Arc<dyn Conn + Send + Sync>>>
    + Send
    + Sync,
>;

struct ConnectionEntry
{
  conn: RwLock<Arc<dyn Conn + Sync + Send>>,
  health: AtomicBool,
}

/// Соединение, состоящие из `n`-нного количества соединенией (потоков), при
/// отправке выбирает соединение (поток) через алгоритм round-robin
///
/// Реализует трейт Conn из webrtc-rs
pub struct BalancedConn
{
  entries: Vec<ConnectionEntry>,
  send_index: AtomicUsize,
  recv_queue: flume::Receiver<(Bytes, SocketAddr)>,
  cancel_token: CancellationToken,
}

impl BalancedConn
{
  /// Создаёт сбалансированное соединение, начинает слушать каждый поток и
  /// кладёт результат слушанья в очередь `recv_queue`
  pub async fn new(
    count: usize,
    factory: ConnFactory,
    cancel_token: CancellationToken,
  ) -> WebRtcResult<Arc<Self>>
  {
    if count == 0 {
      panic!("Connections list cannot be empty");
    }

    // let mut connections: Vec<RwLock<Arc<dyn Conn + Sync + Send>>> =
    // Vec::with_capacity(count); let (sender, receiver) =
    // flume::bounded::<(Bytes, SocketAddr)>(CHANNEL_BUF);
    // let ct = cancel_token.child_token();

    // let mut futures = Vec::with_capacity(count);
    // for _ in 0..count {
    //   futures.push(factory());
    // }
    // let conns_results = join_all(futures).await;
    // for res in conns_results {
    //   connections.push(RwLock::new(res?));
    // }

    // let mut conn_health = Vec::with_capacity(count);
    // for _ in 0..count { conn_health.push(AtomicBool::new(true)); }

    let mut entries: Vec<ConnectionEntry> = Vec::with_capacity(count);
    let (sender, receiver) = flume::bounded::<(Bytes, SocketAddr)>(CHANNEL_BUF);
    let ct = cancel_token.child_token();

    let mut futures = Vec::with_capacity(count);
    for _ in 0..count {
      futures.push(factory());
    }

    let conn_results = join_all(futures).await;
    for res in conn_results {
      let conn = res?;
      entries.push(ConnectionEntry {
        conn: RwLock::new(conn),
        health: AtomicBool::new(true),
      });
    }

    let res = Arc::new(Self {
      cancel_token: ct,
      entries,
      recv_queue: receiver,
      send_index: AtomicUsize::new(0),
    });

    for idx in 0..count {
      let this = res.clone();
      let sender_clone = sender.clone();
      let factory_clone = factory.clone();
      let ct_worker = res.cancel_token.child_token();

      tokio::spawn(async move {
        this
          .worker_conn(idx, factory_clone, sender_clone, ct_worker)
          .await;
      });
    }

    Ok(res)
  }

  async fn worker_conn(
    &self,
    idx: usize,
    factory: ConnFactory,
    sender: flume::Sender<(Bytes, SocketAddr)>,
    ct: CancellationToken,
  )
  {
    let mut buf = vec![0u8; UDP_MTU];
    let mut retry_delay = std::time::Duration::from_secs(MIN_RETRY_DELAY_SECS);

    loop {
      let conn = {
        let lock = self.entries[idx].conn.read();
        lock.clone()
      };

      tokio::select! {
        _ = ct.cancelled() => break,
        res = conn.recv_from(&mut buf) => {
          match res {
            Ok((n, src)) => {
              if !self.entries[idx].health.load(Ordering::Relaxed) {
                self.entries[idx].health.store(true, Ordering::Release);
              }
              let data = Bytes::copy_from_slice(&buf[..n]);
              if sender.send_async((data, src)).await.is_err() { break; }
              retry_delay = std::time::Duration::from_secs(MIN_RETRY_DELAY_SECS);
            },
            Err(e) => {
              self.entries[idx].health.store(false, Ordering::Release);
              warn!(index = idx, "Flow error: {:?}. Reconnecting ({}s)...", e, retry_delay.as_secs());

              tokio::select! {
                _ = ct.cancelled() => return,
                _ = sleep(retry_delay) => {}
              }

              tokio::select! {
                _ = ct.cancelled() => return,
                res = factory() => {
                  match res {
                    Ok(new_conn) => {
                      info!(index = idx, "Reconnected successfully");
                      let entry = &self.entries[idx];
                      let mut lock = entry.conn.write();
                      *lock = new_conn;
                      entry.health.store(true, Ordering::Release);
                      retry_delay = std::time::Duration::from_secs(MIN_RETRY_DELAY_SECS);
                    }
                    Err(reconnect_err) => {
                      error!(index = idx, "Reconnect failed: {:?}", reconnect_err);
                      retry_delay = std::cmp::min(retry_delay * 2, std::time::Duration::from_secs(MAX_RETRY_DELAY_SECS));
                    }
                  };
                }
              }
            }
          }
        }
      }
    }
  }
}

#[async_trait]
impl Conn for BalancedConn
{
  fn as_any(&self) -> &(dyn Any + Send + Sync)
  {
    self
  }

  fn local_addr(&self) -> WebRtcResult<SocketAddr>
  {
    self.entries[0].conn.read().local_addr()
  }
  fn remote_addr(&self) -> Option<SocketAddr>
  {
    self.entries[0].conn.read().remote_addr()
  }

  async fn connect(&self, addr: SocketAddr) -> WebRtcResult<()>
  {
    let current_connections: Vec<_> = self
      .entries
      .iter()
      .map(|entry| entry.conn.read().clone())
      .collect();

    let futures = current_connections.iter().map(|c| c.connect(addr));
    let results = join_all(futures).await;

    for res in results {
      res?;
    }

    Ok(())
  }

  async fn close(&self) -> WebRtcResult<()>
  {
    self.cancel_token.cancel();

    let current_connections: Vec<_> = self
      .entries
      .iter()
      .map(|entry| entry.conn.read().clone())
      .collect();
    let futures = current_connections.iter().map(|c| c.close());

    match tokio::time::timeout(
      std::time::Duration::from_secs(CONNECTION_CLOSING_DELAY),
      join_all(futures),
    )
    .await
    {
      Ok(results) => {
        for res in results {
          if let Err(e) = res {
            error!("Error while closing sub-connection: {:?}", e);
          }
        }
      }
      Err(_) => {
        warn!(
          "Close operation timed out after {} seconds",
          CONNECTION_CLOSING_DELAY
        );
      }
    }

    Ok(())
  }

  async fn send(&self, buf: &[u8]) -> WebRtcResult<usize>
  {
    let count = self.entries.len();

    if count == 0 {
      return Err(WebRtcError::ErrUseClosedNetworkConn);
    }

    let start_idx = self.send_index.fetch_add(1, Ordering::Relaxed) % count;

    for i in 0..count {
      let idx = (start_idx + i) % count;
      debug!(index = idx, "Try to sending...");

      let entry = &self.entries[idx];

      if !entry.health.load(Ordering::Relaxed) {
        continue;
      }

      let conn = {
        let lock = entry.conn.read();
        lock.clone()
      };

      match conn.send(buf).await {
        Ok(n) => return Ok(n),
        Err(e) => {
          entry.health.store(false, Ordering::Release);
          warn!("Sub-connection {} failed to send: {:?}", idx, e);
          if i == count - 1 {
            return Err(e);
          }
          continue;
        }
      };
    }

    Err(WebRtcError::ErrUseClosedNetworkConn)
  }

  async fn send_to(&self, buf: &[u8], target: SocketAddr)
  -> WebRtcResult<usize>
  {
    let count = self.entries.len();

    if count == 0 {
      return Err(WebRtcError::ErrUseClosedNetworkConn);
    }

    let start_idx = self.send_index.fetch_add(1, Ordering::Relaxed) % count;

    for i in 0..count {
      let idx = (start_idx + i) % count;
      debug!(index = idx, "Try to sending...");

      let entry = &self.entries[idx];

      if !entry.health.load(Ordering::Relaxed) {
        continue;
      }

      let conn = {
        let lock = entry.conn.read();
        lock.clone()
      };

      match conn.send_to(buf, target).await {
        Ok(n) => return Ok(n),
        Err(e) => {
          entry.health.store(false, Ordering::Release);
          warn!("Sub-connection {} failed to send: {:?}", idx, e);
          if i == count - 1 {
            return Err(e);
          }
          continue;
        }
      };
    }

    Err(WebRtcError::ErrUseClosedNetworkConn)
  }

  async fn recv_from(&self, buf: &mut [u8])
  -> WebRtcResult<(usize, SocketAddr)>
  {
    match self.recv_queue.recv_async().await {
      Ok((data, addr)) => {
        if data.len() > buf.len() {
          return Err(WebRtcError::ErrBufferShort);
        }

        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);

        Ok((n, addr))
      }
      Err(_) => Err(WebRtcError::ErrClosedListener),
    }
  }

  async fn recv(&self, buf: &mut [u8]) -> WebRtcResult<usize>
  {
    let (n, _) = self.recv_from(buf).await?;
    Ok(n)
  }
}

impl Drop for BalancedConn
{
  fn drop(&mut self)
  {
    self.cancel_token.cancel();
  }
}
