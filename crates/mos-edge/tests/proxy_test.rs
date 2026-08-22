use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use m_os_edge::{EdgeProxy, EdgeRouter, RouteTarget};
use mos_core::InstanceId;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

async fn start_mock_backend(port: u16, message: &'static str) -> tokio::task::JoinHandle<()> {
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
    let listener = TcpListener::bind(addr).await.unwrap();

    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = listener.accept().await {
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(move |_req: Request<hyper::body::Incoming>| async move {
                                Ok::<_, hyper::Error>(
                                    Response::builder()
                                        .status(StatusCode::OK)
                                        .header("Content-Type", "text/plain")
                                        .body(Full::new(Bytes::from(message)))
                                        .unwrap(),
                                )
                            }),
                        )
                        .await;
                });
            }
        }
    })
}

#[tokio::test]
async fn test_edge_proxy_routing_and_forwarding() {
    let backend_port = 19123;
    let _backend_handle = start_mock_backend(backend_port, "Hello from Vibe App!").await;

    let router = EdgeRouter::new();
    let target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "127.0.0.1".to_string(),
        port: backend_port,
        is_suspended: false,
    };
    router.register("vibe-app.mos.local", target);

    let proxy = Arc::new(EdgeProxy::new(router, None));
    let proxy_port = 19124;
    let proxy_addr: SocketAddr = format!("127.0.0.1:{}", proxy_port).parse().unwrap();

    let proxy_clone = Arc::clone(&proxy);
    tokio::spawn(async move {
        let _ = proxy_clone.run_server(proxy_addr).await;
    });

    // Wait for proxy server to start listening
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();

    // 1. Success forwarding request
    let res = client
        .get(format!("http://127.0.0.1:{}", proxy_port))
        .header("Host", "vibe-app.mos.local")
        .send()
        .await
        .expect("Failed to send request to proxy");

    assert_eq!(res.status(), 200);
    let text = res.text().await.unwrap();
    assert_eq!(text, "Hello from Vibe App!");

    // 2. 404 for unrouted host
    let not_found_res = client
        .get(format!("http://127.0.0.1:{}", proxy_port))
        .header("Host", "unknown-app.mos.local")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(not_found_res.status(), 404);
}
