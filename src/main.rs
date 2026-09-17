mod config;
mod proxy;
mod tls;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "proxy.toml".to_string());
    let config = Config::load(Path::new(&config_path))
        .with_context(|| format!("loading config from {config_path}"))?;

    let backends: HashMap<String, SocketAddr> = config
        .hosts
        .iter()
        .map(|h| (h.domain.clone(), h.backend))
        .collect();
    let backends = Arc::new(backends);

    let server_config = tls::build_server_config(&config.hosts)?;
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("binding to {}", config.listen))?;
    tracing::info!(addr = %config.listen, hosts = config.hosts.len(), "proxrs listening");

    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(pair) => pair,
            Err(err) => {
                tracing::warn!(%err, "accept failed");
                continue;
            }
        };

        let acceptor = acceptor.clone();
        let backends = backends.clone();

        tokio::spawn(async move {
            if let Err(err) = proxy::handle_connection(stream, peer_addr, acceptor, backends).await
            {
                tracing::warn!(%err, %peer_addr, "connection error");
            }
        });
    }
}
