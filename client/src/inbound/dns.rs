use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use hickory_resolver::{
  Resolver,
  config::{NameServerConfig, ResolverConfig},
  name_server::{GenericConnector, TokioConnectionProvider},
  proto::{runtime::TokioRuntimeProvider, xfer::Protocol},
};
use reqwest::dns::Resolve;

const YANDEX_DNS_FIRST_IP: &str = "77.88.8.8";
const YANDEX_DNS_SECOND_IP: &str = "77.88.8.1";

#[derive(Debug)]
pub struct YandexDnsResolver
{
  pub inner: Arc<Resolver<GenericConnector<TokioRuntimeProvider>>>,
}

impl Resolve for YandexDnsResolver
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

/// Нужен для работы под белыми списками, если в самой системе работают
/// неяндексовские или непровайдеровские DNS-сервера, которых нет в белых
/// списках
///
/// TODO: Реализовать резолвинг peer (сервера назначения), если задан не IP
/// адрес, а домен
pub fn configure_yandex_dns() -> Result<YandexDnsResolver>
{
  let mut config = ResolverConfig::new();

  let dns_servernames = [YANDEX_DNS_FIRST_IP, YANDEX_DNS_SECOND_IP];

  for ip in dns_servernames {
    let addr = SocketAddr::new(ip.parse()?, 53);
    config.add_name_server(NameServerConfig::new(addr, Protocol::Udp));
  }

  let resolver: Resolver<GenericConnector<TokioRuntimeProvider>> =
    Resolver::builder_with_config(config, TokioConnectionProvider::default())
      .build();

  Ok(YandexDnsResolver {
    inner: Arc::new(resolver),
  })
}
