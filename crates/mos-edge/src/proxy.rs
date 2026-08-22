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
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
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
}

fn debug_error(err: impl std::fmt::Debug) {
    tracing::debug!("Connection closed: {:?}", err);
}
