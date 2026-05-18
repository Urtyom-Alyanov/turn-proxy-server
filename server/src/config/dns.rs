use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use hickory_resolver::{
  Resolver,
  name_server::{GenericConnector, TokioConnectionProvider},
  proto::runtime::TokioRuntimeProvider,
  system_conf::read_system_conf,
};
use reqwest::dns::Resolve;

/// Конфигурируем резолвер, который будет использовать системные DNS-сервера
/// Нужен для работы под Docker'ом, если внутри контейнера настроены системный
/// DNS-сервер, который предоставляет резолвинг для доменных имён, используемых
/// в конфигурации (например, для proxy-into)
///
/// Пример:
/// ```toml
/// [common]
/// peer_addr = "wireguard:56040"
/// ```
pub fn configure_system_dns() -> Result<SysDnsResolver>
{
  let (config, options) = read_system_conf()?;

  let resolver: Resolver<GenericConnector<TokioRuntimeProvider>> =
    Resolver::builder_with_config(config, TokioConnectionProvider::default())
      .with_options(options)
      .build();

  Ok(SysDnsResolver {
    inner: Arc::new(resolver),
  })
}

#[derive(Debug)]
pub struct SysDnsResolver
{
  pub inner: Arc<Resolver<GenericConnector<TokioRuntimeProvider>>>,
}

impl Resolve for SysDnsResolver
{
  fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving
  {
    let resolver = Arc::clone(&self.inner);
    Box::pin(async move {
      let lookup = resolver.lookup_ip(name.as_str()).await?;
      let addrs: Box<dyn Iterator<Item = SocketAddr> + Send> =
        Box::new(lookup.into_iter().map(|ip| SocketAddr::new(ip, 0)));
      Ok(addrs)
    })
  }
}
