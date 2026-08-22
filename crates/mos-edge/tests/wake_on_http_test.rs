use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use m_os_edge::{EdgeProxy, EdgeRouter, RouteTarget};
use mos_core::InstanceId;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::Instant;

#[tokio::test]
async fn test_wake_on_http_end_to_end_flow() {
    let backend_port = 19230;
    let proxy_port = 19231;

    let is_backend_alive = Arc::new(AtomicBool::new(false));
    let backend_alive_clone = Arc::clone(&is_backend_alive);

    // Wake-up channel simulation
    let (wake_tx, mut wake_rx) = mpsc::channel::<String>(100);

    // Background listener simulating Orchestrator resuming the VM
    tokio::spawn(async move {
        while let Some(domain) = wake_rx.recv().await {
            println!(
                "⚡ Orchestrator received wake request for domain: {}",
                domain
            );
            // Simulate 10ms VM resume latency
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;

            // Start backend server
            let addr: SocketAddr = format!("127.0.0.1:{}", backend_port).parse().unwrap();
            let listener = TcpListener::bind(addr).await.unwrap();
            backend_alive_clone.store(true, Ordering::SeqCst);

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
                                            .header("Content-Type", "application/json")
                                            .body(Full::new(Bytes::from(
                                                r#"{"status":"ok","message":"Woken up on demand!"}"#,
                                            )))
                                            .unwrap(),
                                    )
                                }),
                            )
                            .await;
                    });
                }
            });
        }
    });

    let router = EdgeRouter::new();
    let target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "127.0.0.1".to_string(),
        port: backend_port,
        is_suspended: true, // Initially Suspended!
    };
    router.register("wake-app.mos.local", target);

    let proxy = Arc::new(EdgeProxy::new(router, Some(wake_tx)));
    let proxy_addr: SocketAddr = format!("127.0.0.1:{}", proxy_port).parse().unwrap();

    let proxy_clone = Arc::clone(&proxy);
    tokio::spawn(async move {
        let _ = proxy_clone.run_server(proxy_addr).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();

    let start = Instant::now();
    let res = client
        .get(format!("http://127.0.0.1:{}", proxy_port))
        .header("Host", "wake-app.mos.local")
        .send()
        .await
        .expect("Failed to send Wake-on-HTTP request");
    let total_elapsed = start.elapsed();

    println!("⚡ Total Wake-on-HTTP Request Latency: {:?}", total_elapsed);
    assert_eq!(res.status(), 200);

    let body_json: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body_json["status"], "ok");
    assert_eq!(body_json["message"], "Woken up on demand!");
    println!("✅ End-to-End Wake-on-HTTP test passed!");
}
