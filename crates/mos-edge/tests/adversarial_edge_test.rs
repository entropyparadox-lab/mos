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

#[tokio::test]
async fn test_adversarial_backend_crash_handling() {
    let router = EdgeRouter::new();
    let target = RouteTarget::new(InstanceId::new(), "127.0.0.1", 19999, false);
    router.register("dead-app.mos.local", target);

    let proxy = Arc::new(EdgeProxy::new(router, None));
    let proxy_port = 19998;
    let proxy_addr: SocketAddr = format!("127.0.0.1:{}", proxy_port).parse().unwrap();

    let proxy_clone = Arc::clone(&proxy);
    tokio::spawn(async move {
        let _ = proxy_clone.run_server(proxy_addr).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://127.0.0.1:{}", proxy_port))
        .header("Host", "dead-app.mos.local")
        .send()
        .await
        .expect("Failed to send request");

    // Must return 502 Bad Gateway with JSON error structure
    assert_eq!(res.status(), 502);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"], "Bad Gateway");
    println!("🛡️ Handled dead backend with clean 502 Bad Gateway response");
}

#[tokio::test]
async fn test_adversarial_concurrent_request_flooding() {
    let backend_port = 19995;
    let proxy_port = 19994;

    // Start fast echo backend
    let addr: SocketAddr = format!("127.0.0.1:{}", backend_port).parse().unwrap();
    let listener = TcpListener::bind(addr).await.unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            tokio::spawn(async move {
                let _ = http1::Builder::new()
                    .serve_connection(
                        io,
                        service_fn(move |_req: Request<hyper::body::Incoming>| async move {
                            Ok::<_, hyper::Error>(
                                Response::builder()
                                    .status(StatusCode::OK)
                                    .body(Full::new(Bytes::from("pong")))
                                    .unwrap(),
                            )
                        }),
                    )
                    .await;
            });
        }
    });

    let router = EdgeRouter::new();
    let target = RouteTarget::new(InstanceId::new(), "127.0.0.1", backend_port, false);
    router.register("flood-app.mos.local", target);

    let proxy = Arc::new(EdgeProxy::new(router, None));
    let proxy_addr: SocketAddr = format!("127.0.0.1:{}", proxy_port).parse().unwrap();
    let proxy_clone = Arc::clone(&proxy);
    tokio::spawn(async move {
        let _ = proxy_clone.run_server(proxy_addr).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Send 50 concurrent requests
    let client = reqwest::Client::new();
    let mut handles = Vec::new();
    for i in 0..50 {
        let c = client.clone();
        let handle = tokio::spawn(async move {
            let res = c
                .get(format!("http://127.0.0.1:{}", proxy_port))
                .header("Host", "flood-app.mos.local")
                .header("X-Request-Id", i.to_string())
                .send()
                .await
                .unwrap();
            assert_eq!(res.status(), 200);
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }
    println!("🛡️ 50 Concurrent flooded requests handled successfully with zero errors!");
}
