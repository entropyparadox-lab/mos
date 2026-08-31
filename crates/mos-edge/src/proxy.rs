use crate::router::{EdgeRouter, RouteTarget};
use anyhow::Result;
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::HOST;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info, warn};

pub type ResponseBody = BoxBody<Bytes, hyper::Error>;

fn full_body<T: Into<Bytes>>(chunk: T) -> ResponseBody {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

pub struct EdgeProxy {
    router: EdgeRouter,
    wake_tx: Option<mpsc::Sender<String>>,
}

impl EdgeProxy {
    pub fn new(router: EdgeRouter, wake_tx: Option<mpsc::Sender<String>>) -> Self {
        Self { router, wake_tx }
    }

    pub async fn handle_request(
        &self,
        req: Request<Incoming>,
    ) -> Result<Response<ResponseBody>, hyper::Error> {
        let host_header = req
            .headers()
            .get(HOST)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();

        let host = host_header.split(':').next().unwrap_or(&host_header);

        let target = self.router.resolve(host);
        let target = match target {
            Some(t) => t,
            None => {
                // Try wildcard or fallback
                let fallback = host
                    .split('.')
                    .next()
                    .and_then(|sub| self.router.resolve(sub));
                match fallback {
                    Some(t) => t,
                    None => {
                        let body = serde_json::json!({
                            "error": "Not Found",
                            "message": format!("No MOS instance routed for host '{}'", host),
                            "status": 404
                        });
                        return Ok(Response::builder()
                            .status(StatusCode::NOT_FOUND)
                            .header("Content-Type", "application/json")
                            .body(full_body(body.to_string()))
                            .unwrap());
                    }
                }
            }
        };

        // If target is suspended, trigger wake-up
        if target.is_suspended {
            if let Some(wake) = &self.wake_tx {
                info!(
                    host = %host,
                    wake_mode = ?target.wake_mode,
                    "Waking up suspended instance via Wake-on-HTTP"
                );
                let _ = wake.send(host.to_string()).await;

                // WakeMode에 따른 버퍼링 슬립 시간 조절
                let sleep_ms = match target.wake_mode {
                    crate::router::WakeMode::SnapshotResume => 30,
                    crate::router::WakeMode::ColdBoot => 50,
                };
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms)).await;
            }
        }

        // Forward request to backend
        match self.forward(req, &target).await {
            Ok(res) => Ok(res),
            Err(err) => {
                error!(error = %err, target = ?target, "Proxy forwarding failed");
                let body = serde_json::json!({
                    "error": "Bad Gateway",
                    "message": format!("Failed to connect to MOS backend: {}", err),
                    "status": 502
                });
                Ok(Response::builder()
                    .status(StatusCode::BAD_GATEWAY)
                    .header("Content-Type", "application/json")
                    .body(full_body(body.to_string()))
                    .unwrap())
            }
        }
    }

    async fn forward(
        &self,
        req: Request<Incoming>,
        target: &RouteTarget,
    ) -> Result<Response<ResponseBody>> {
        let addr = format!("{}:{}", target.host, target.port);
        let stream = TcpStream::connect(&addr).await?;
        let io = TokioIo::new(stream);

        let (mut sender, conn) = hyper::client::conn::http1::handshake(io).await?;
        tokio::spawn(async move {
            if let Err(err) = conn.await {
                warn!("Backend connection closed with error: {:?}", err);
            }
        });

        let (parts, body) = req.into_parts();
        let body_bytes = body.collect().await?.to_bytes();

        let mut client_req = Request::builder().method(parts.method).uri(parts.uri);

        for (k, v) in parts.headers.iter() {
            if k != hyper::header::HOST {
                client_req = client_req.header(k, v);
            }
        }
        client_req = client_req.header(hyper::header::HOST, &target.host);
        client_req = client_req.header("X-Forwarded-For", "127.0.0.1");
        client_req = client_req.header("X-Forwarded-Proto", "http");

        let forward_req = client_req.body(Full::new(body_bytes))?;
        let res = sender.send_request(forward_req).await?;

        let (res_parts, res_body) = res.into_parts();
        let boxed_body = res_body.map_err(|e| e).boxed();

        Ok(Response::from_parts(res_parts, boxed_body))
    }

    pub async fn run_server(self: Arc<Self>, bind_addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(bind_addr).await?;
        info!("MOS Edge Ingress Proxy listening on http://{}", bind_addr);

        loop {
            let (stream, _remote_addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("Failed to accept incoming connection: {:?}", e);
                    continue;
                }
            };

            let proxy = Arc::clone(&self);
            let io = TokioIo::new(stream);

            tokio::spawn(async move {
                let service = service_fn(move |req| {
                    let proxy = Arc::clone(&proxy);
                    async move { proxy.handle_request(req).await }
                });

                let auto_server = auto::Builder::new(TokioExecutor::new());
                if let Err(err) = auto_server
                    .serve_connection_with_upgrades(io, service)
                    .await
                {
                    debug_error(err);
                }
            });
        }
    }

    pub async fn run_tls_server(
        self: Arc<Self>,
        bind_addr: SocketAddr,
        cert_path: PathBuf,
        key_path: PathBuf,
    ) -> Result<()> {
        let tls_config = load_tls_config(&cert_path, &key_path)?;
        let acceptor = TlsAcceptor::from(tls_config);
        let listener = TcpListener::bind(bind_addr).await?;
        info!(
            "MOS Edge TLS Ingress Proxy listening on https://{}",
            bind_addr
        );

        loop {
            let (stream, _remote_addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!("Failed to accept TLS incoming connection: {:?}", e);
                    continue;
                }
            };

            let acceptor = acceptor.clone();
            let proxy = Arc::clone(&self);

            tokio::spawn(async move {
                let tls_stream = match acceptor.accept(stream).await {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::debug!("TLS handshake failed: {:?}", e);
                        return;
                    }
                };

                let io = TokioIo::new(tls_stream);
                let service = service_fn(move |req| {
                    let proxy = Arc::clone(&proxy);
                    async move { proxy.handle_request(req).await }
                });

                let auto_server = auto::Builder::new(TokioExecutor::new());
                if let Err(err) = auto_server
                    .serve_connection_with_upgrades(io, service)
                    .await
                {
                    debug_error(err);
                }
            });
        }
    }
}

#[derive(Debug)]
pub struct CustomSniResolver {
    sni: tokio_rustls::rustls::server::ResolvesServerCertUsingSni,
    fallback: Arc<tokio_rustls::rustls::sign::CertifiedKey>,
}

impl tokio_rustls::rustls::server::ResolvesServerCert for CustomSniResolver {
    fn resolve(
        &self,
        client_hello: tokio_rustls::rustls::server::ClientHello,
    ) -> Option<Arc<tokio_rustls::rustls::sign::CertifiedKey>> {
        if let Some(key) = self.sni.resolve(client_hello) {
            Some(key)
        } else {
            Some(self.fallback.clone())
        }
    }
}

pub fn load_tls_config(cert_path: &Path, key_path: &Path) -> Result<Arc<ServerConfig>> {
    let certfile = File::open(cert_path)?;
    let mut reader = BufReader::new(certfile);
    let certs: Vec<_> = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;

    let keyfile = File::open(key_path)?;
    let mut reader = BufReader::new(keyfile);
    let key = rustls_pemfile::private_key(&mut reader)?
        .ok_or_else(|| anyhow::anyhow!("No private key found in {}", key_path.display()))?;

    let primary_signing_key =
        tokio_rustls::rustls::crypto::aws_lc_rs::sign::any_supported_type(&key)?;
    let primary_certified = Arc::new(tokio_rustls::rustls::sign::CertifiedKey::new(
        certs,
        primary_signing_key,
    ));

    let mut sni = tokio_rustls::rustls::server::ResolvesServerCertUsingSni::new();
    let _ = sni.add("*.mos.local", (*primary_certified).clone());
    let _ = sni.add("mos.local", (*primary_certified).clone());

    // Check if Tailscale cert exists in same directory
    let ts_cert = cert_path
        .parent()
        .unwrap_or(Path::new(""))
        .join("tailscale.crt");
    let ts_key = cert_path
        .parent()
        .unwrap_or(Path::new(""))
        .join("tailscale.key");
    if ts_cert.exists() && ts_key.exists() {
        if let (Ok(f_c), Ok(f_k)) = (File::open(&ts_cert), File::open(&ts_key)) {
            let mut r_c = BufReader::new(f_c);
            let mut r_k = BufReader::new(f_k);
            if let (Ok(c_list), Ok(Some(k))) = (
                rustls_pemfile::certs(&mut r_c).collect::<Result<Vec<_>, _>>(),
                rustls_pemfile::private_key(&mut r_k),
            ) {
                if let Ok(sign_key) =
                    tokio_rustls::rustls::crypto::aws_lc_rs::sign::any_supported_type(&k)
                {
                    let ts_certified = Arc::new(tokio_rustls::rustls::sign::CertifiedKey::new(
                        c_list, sign_key,
                    ));
                    let _ = sni.add(
                        "YOUR_TAILSCALE_HOST.ts.net",
                        (*ts_certified).clone(),
                    );
                }
            }
        }
    }

    let resolver = CustomSniResolver {
        sni,
        fallback: primary_certified,
    };

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(resolver));

    Ok(Arc::new(config))
}

fn debug_error(err: impl std::fmt::Debug) {
    tracing::debug!("Connection closed: {:?}", err);
}
