use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use rustls::ServerConfig;
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

use crate::config::HostConfig;

#[derive(Debug)]
struct HostResolver {
    certs: HashMap<String, Arc<CertifiedKey>>,
}

impl ResolvesServerCert for HostResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        let name = client_hello.server_name()?;
        self.certs.get(name).cloned()
    }
}

pub fn build_server_config(hosts: &[HostConfig]) -> anyhow::Result<ServerConfig> {
    let mut certs = HashMap::new();
    for host in hosts {
        let key = load_certified_key(&host.cert, &host.key)
            .with_context(|| format!("loading TLS cert/key for {}", host.domain))?;
        certs.insert(host.domain.clone(), Arc::new(key));
    }

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(HostResolver { certs }));

    Ok(config)
}

fn load_certified_key(cert_path: &Path, key_path: &Path) -> anyhow::Result<CertifiedKey> {
    let cert_file = std::fs::File::open(cert_path)
        .with_context(|| format!("opening cert file {}", cert_path.display()))?;
    let mut cert_reader = std::io::BufReader::new(cert_file);
    let certs = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("parsing cert file {}", cert_path.display()))?;
    anyhow::ensure!(
        !certs.is_empty(),
        "no certificates found in {}",
        cert_path.display()
    );

    let key_file = std::fs::File::open(key_path)
        .with_context(|| format!("opening key file {}", key_path.display()))?;
    let mut key_reader = std::io::BufReader::new(key_file);
    let key = rustls_pemfile::private_key(&mut key_reader)
        .with_context(|| format!("parsing key file {}", key_path.display()))?
        .with_context(|| format!("no private key found in {}", key_path.display()))?;

    let signing_key = rustls::crypto::ring::sign::any_supported_type(&key)
        .with_context(|| format!("unsupported private key type in {}", key_path.display()))?;

    Ok(CertifiedKey::new(certs, signing_key))
}
