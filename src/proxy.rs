use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt, Empty, combinators::BoxBody};
use hyper::body::Incoming;
use hyper::header::HeaderValue;
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type ProxyBody = BoxBody<Bytes, BoxError>;

pub async fn handle_connection(
    stream: TcpStream,
    peer_addr: SocketAddr,
    acceptor: TlsAcceptor,
    backends: Arc<HashMap<String, SocketAddr>>,
) -> anyhow::Result<()> {
    let tls_stream = acceptor.accept(stream).await?;

    let domain = tls_stream
        .get_ref()
        .1
        .server_name()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("client did not send SNI"))?;

    let backend = *backends
        .get(&domain)
        .ok_or_else(|| anyhow::anyhow!("no backend configured for host {domain}"))?;

    let io = TokioIo::new(tls_stream);

    server_http1::Builder::new()
        .serve_connection(io, service_fn(move |req| forward(req, backend, peer_addr)))
        .await?;

    Ok(())
}

async fn forward(
    mut req: Request<Incoming>,
    backend: SocketAddr,
    peer_addr: SocketAddr,
) -> Result<Response<ProxyBody>, Infallible> {
    req.headers_mut().insert(
        "x-forwarded-for",
        HeaderValue::from_str(&peer_addr.ip().to_string())
            .unwrap_or_else(|_| HeaderValue::from_static("unknown")),
    );
    req.headers_mut()
        .insert("x-forwarded-proto", HeaderValue::from_static("https"));

    let method = req.method().clone();
    let path = req.uri().clone();

    match send_to_backend(req, backend).await {
        Ok(resp) => {
            tracing::info!(%method, %path, %backend, status = %resp.status(), "proxied");
            let (parts, body) = resp.into_parts();
            Ok(Response::from_parts(
                parts,
                body.map_err(|e| Box::new(e) as BoxError).boxed(),
            ))
        }
        Err(err) => {
            tracing::warn!(%method, %path, %backend, %err, "backend request failed");
            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(empty_body())
                .expect("static response is valid"))
        }
    }
}

async fn send_to_backend(
    req: Request<Incoming>,
    backend: SocketAddr,
) -> anyhow::Result<Response<Incoming>> {
    let stream = TcpStream::connect(backend).await?;
    let io = TokioIo::new(stream);

    let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
    tokio::spawn(async move {
        if let Err(err) = conn.await {
            tracing::warn!(%err, "backend connection closed with error");
        }
    });

    let resp = sender.send_request(req).await?;
    Ok(resp)
}

fn empty_body() -> ProxyBody {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}
