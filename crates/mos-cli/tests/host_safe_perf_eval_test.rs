use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use mos_cluster::ConsistentHashRing;
use mos_core::{
    BillingEngine, BillingRate, CreditAccount, Ed25519AuthManager, InstanceConfig, InstanceId,
    RbacTokenPayload, Role, TenantId, TenantManager, TenantNamespace, UsageMetric,
};
use mos_edge::{EbpfXdpFilter, EdgeProxy, EdgeRouter, RouteTarget, TraceContext};
use mos_orchestrator::{
    GpuDevice, GpuPoolManager, MicroVmInstance, SnapshotEngine, UffdSnapshotEngine,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;

fn percentile(sorted_samples: &[f64], pct: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    let idx = ((sorted_samples.len() as f64) * (pct / 100.0)).ceil() as usize;
    sorted_samples[idx.min(sorted_samples.len()) - 1]
}

#[derive(Debug, Clone)]
struct Stats {
    min: f64,
    max: f64,
    mean: f64,
    p50: f64,
    p95: f64,
    #[allow(dead_code)]
    p99: f64,
    count: usize,
}

impl Stats {
    fn from_samples(mut samples: Vec<f64>) -> Self {
        if samples.is_empty() {
            return Self {
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                p50: 0.0,
                p95: 0.0,
                p99: 0.0,
                count: 0,
            };
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let count = samples.len();
        let sum: f64 = samples.iter().sum();
        let mean = sum / count as f64;
        let min = samples[0];
        let max = samples[count - 1];
        let p50 = percentile(&samples, 50.0);
        let p95 = percentile(&samples, 95.0);
        let p99 = percentile(&samples, 99.0);

        Self {
            min,
            max,
            mean,
            p50,
            p95,
            p99,
            count,
        }
    }
}

fn find_mos_root() -> PathBuf {
    let mut curr = std::env::current_dir().unwrap();
    while !curr.join("crates").exists() {
        if !curr.pop() {
            break;
        }
    }
    curr
}

#[tokio::test]
async fn test_full_host_safe_performance_evaluation() {
    println!("\n===============================================================================");
    println!("       🦀 MOS Zero-Impact Full-Track Performance Benchmark Evaluation          ");
    println!("===============================================================================");

    let mos_root = find_mos_root();
    let firecracker_bin = mos_root.join("bin/firecracker");
    let kernel_path = mos_root.join("runtime/kernels/vmlinux.bin");
    let base_rootfs = mos_root.join("runtime/base-rootfs/bionic.rootfs.ext4");

    if !firecracker_bin.exists() || !kernel_path.exists() || !base_rootfs.exists() {
        println!("Skipping performance benchmark: Firecracker/Kernel/RootFS binaries not found in this environment.");
        return;
    }

    let temp_root = tempfile::tempdir().expect("Failed to create tempdir");
    let temp_path = temp_root.path();

    // =========================================================================
    // 1. TRACK A: MicroVM Lifecycle Micro-benchmarks (10 Samples)
    // =========================================================================
    println!("\n--- [Track A: MicroVM Lifecycle Latency & Scale-to-Zero] ---");
    let track_a_samples = 10;
    let mut cold_boot_latencies = Vec::new();
    let mut snapshot_latencies = Vec::new();
    let mut resume_latencies = Vec::new();
    let mut uffd_compress_latencies = Vec::new();
    let mut uffd_decompress_latencies = Vec::new();
    let mut zstd_compress_ratios = Vec::new();

    let uffd_engine = UffdSnapshotEngine::new(firecracker_bin.clone());
    let uffd_temp = temp_path.join("uffd_bench");
    std::fs::create_dir_all(&uffd_temp).unwrap();

    // 1.1 MicroVM Boot, Snapshot, Resume & UFFD
    for i in 0..track_a_samples {
        let run_dir = temp_path.join(format!("track_a_vm_{}", i));
        let snap_dir = temp_path.join(format!("track_a_snap_{}", i));
        std::fs::create_dir_all(&run_dir).unwrap();
        std::fs::create_dir_all(&snap_dir).unwrap();

        let vm_rootfs = run_dir.join("rootfs.ext4");
        tokio::fs::copy(&base_rootfs, &vm_rootfs).await.unwrap();

        let config = InstanceConfig::new(format!("bench-vm-{}", i), kernel_path.clone(), vm_rootfs);

        // Cold Boot
        let t0 = Instant::now();
        let inst = MicroVmInstance::boot(
            &firecracker_bin,
            run_dir.join("fc.sock"),
            config.clone(),
            "console=ttyS0 reboot=k panic=1 pci=off init=/bin/sh",
        )
        .await
        .expect("MicroVM cold boot failed");
        let cold_boot_ms = t0.elapsed().as_secs_f64() * 1000.0;
        cold_boot_latencies.push(cold_boot_ms);

        // Snapshot
        let snap_engine = SnapshotEngine::new(firecracker_bin.clone());
        let t1 = Instant::now();
        let artifacts = snap_engine
            .snapshot_and_stop(inst, &snap_dir)
            .await
            .expect("Snapshot creation failed");
        let snap_ms = t1.elapsed().as_secs_f64() * 1000.0;
        snapshot_latencies.push(snap_ms);

        // Fast Resume
        let (mut resumed, resume_dur) = snap_engine
            .resume_from_snapshot(run_dir.join("fc_resumed.sock"), config, &artifacts)
            .await
            .expect("Fast resume failed");
        let resume_ms = resume_dur.as_secs_f64() * 1000.0;
        resume_latencies.push(resume_ms);

        let _ = resumed.process.kill().await;

        // UFFD ZSTD Compression & Decompression using actual snapshot artifacts
        let uffd_out_dir = uffd_temp.join(format!("snap_{}", i));
        let t_comp = Instant::now();
        let compressed = uffd_engine
            .compress_snapshot(&artifacts, &uffd_out_dir)
            .expect("UFFD compression failed");
        let comp_ms = t_comp.elapsed().as_secs_f64() * 1000.0;
        uffd_compress_latencies.push(comp_ms);
        zstd_compress_ratios.push(compressed.compression_ratio * 100.0);

        let dest_decomp = uffd_temp.join(format!("decomp_{}.mem", i));
        let t_decomp = Instant::now();
        let _ = uffd_engine
            .decompress_snapshot(&compressed, &dest_decomp)
            .expect("UFFD decompression failed");
        let decomp_ms = t_decomp.elapsed().as_secs_f64() * 1000.0;
        uffd_decompress_latencies.push(decomp_ms);
    }

    let stats_cold_boot = Stats::from_samples(cold_boot_latencies);
    let stats_snap = Stats::from_samples(snapshot_latencies);
    let stats_resume = Stats::from_samples(resume_latencies);
    let stats_uffd_comp = Stats::from_samples(uffd_compress_latencies);
    let stats_uffd_decomp = Stats::from_samples(uffd_decompress_latencies);
    let stats_zstd_ratio = Stats::from_samples(zstd_compress_ratios);

    println!("  • Cold Boot Latency:         Mean={:.2}ms | Min={:.2}ms | P50={:.2}ms | P95={:.2}ms | Max={:.2}ms", stats_cold_boot.mean, stats_cold_boot.min, stats_cold_boot.p50, stats_cold_boot.p95, stats_cold_boot.max);
    println!("  • Fast Resume Latency:       Mean={:.2}ms | Min={:.2}ms | P50={:.2}ms | P95={:.2}ms | Max={:.2}ms", stats_resume.mean, stats_resume.min, stats_resume.p50, stats_resume.p95, stats_resume.max);
    println!("  • Snapshot Creation Latency: Mean={:.2}ms | Min={:.2}ms | P50={:.2}ms | P95={:.2}ms | Max={:.2}ms", stats_snap.mean, stats_snap.min, stats_snap.p50, stats_snap.p95, stats_snap.max);
    println!(
        "  • UFFD ZSTD Compression:     Mean={:.2}ms | ZSTD Ratio={:.1}%",
        stats_uffd_comp.mean, stats_zstd_ratio.mean
    );
    println!(
        "  • UFFD ZSTD Decompression:   Mean={:.2}ms | P95={:.2}ms",
        stats_uffd_decomp.mean, stats_uffd_decomp.p95
    );

    // =========================================================================
    // 2. TRACK B: Ingress Edge Proxy & HTTP Latency (200 Requests)
    // =========================================================================
    println!("\n--- [Track B: Edge Ingress Proxy & HTTP Latency] ---");

    // 2.1 Setup Mock Upstream Service using Hyper
    let upstream_port = 19280;
    let upstream_addr: SocketAddr = format!("127.0.0.1:{}", upstream_port).parse().unwrap();
    let upstream_listener = TcpListener::bind(upstream_addr).await.unwrap();

    let _mock_backend = tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = upstream_listener.accept().await {
                let io = TokioIo::new(stream);
                tokio::spawn(async move {
                    let _ = http1::Builder::new()
                        .serve_connection(
                            io,
                            service_fn(|_req: Request<hyper::body::Incoming>| async move {
                                Ok::<_, hyper::Error>(
                                    Response::builder()
                                        .status(StatusCode::OK)
                                        .header("Content-Type", "application/json")
                                        .body(Full::new(Bytes::from(
                                            r#"{"status":"healthy","engine":"mos-upstream"}"#,
                                        )))
                                        .unwrap(),
                                )
                            }),
                        )
                        .await;
                });
            }
        }
    });

    // 2.2 Setup MOS Edge Proxy
    let router = EdgeRouter::new();
    let target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "127.0.0.1".to_string(),
        port: upstream_port,
        is_suspended: false,
    };
    router.register("bench.mos.local", target);

    let proxy = Arc::new(EdgeProxy::new(router.clone(), None));
    let proxy_port = 19281;
    let proxy_addr: SocketAddr = format!("127.0.0.1:{}", proxy_port).parse().unwrap();

    let proxy_clone = Arc::clone(&proxy);
    tokio::spawn(async move {
        let _ = proxy_clone.run_server(proxy_addr).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2.3 Run 200 HTTP Requests across 4 concurrent clients
    let http_client = reqwest::Client::builder()
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build()
        .unwrap();

    let total_reqs = 200;
    let concurrency = 4;
    let reqs_per_task = total_reqs / concurrency;

    let t_proxy_start = Instant::now();
    let mut handles = Vec::new();

    for _ in 0..concurrency {
        let client = http_client.clone();
        let target_url = format!("http://127.0.0.1:{}/api/health", proxy_port);
        let handle = tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(reqs_per_task);
            for _ in 0..reqs_per_task {
                let req_t0 = Instant::now();
                let resp = client
                    .get(&target_url)
                    .header("Host", "bench.mos.local")
                    .header("X-MOS-Trace", "1")
                    .send()
                    .await
                    .expect("Proxy request failed");
                assert_eq!(resp.status(), 200);
                let rtt_ms = req_t0.elapsed().as_secs_f64() * 1000.0;
                latencies.push(rtt_ms);
            }
            latencies
        });
        handles.push(handle);
    }

    let mut proxy_latencies = Vec::with_capacity(total_reqs);
    for h in handles {
        let mut l = h.await.unwrap();
        proxy_latencies.append(&mut l);
    }
    let total_elapsed = t_proxy_start.elapsed();
    let rps = total_reqs as f64 / total_elapsed.as_secs_f64();
    let stats_proxy = Stats::from_samples(proxy_latencies);

    // 2.4 W3C Distributed Tracing Benchmark (1,000 iterations)
    let t_trace = Instant::now();
    let trace_count = 1000;
    for _ in 0..trace_count {
        let trace = TraceContext::new();
        let trace_header = trace.to_traceparent();
        let parsed = TraceContext::from_traceparent(&trace_header).unwrap();
        let _child = parsed.child_span();
    }
    let trace_us = (t_trace.elapsed().as_secs_f64() * 1_000_000.0) / trace_count as f64;

    // 2.5 Weighted Canary Decision Latency (10,000 decisions)
    let canary_router = EdgeRouter::new();
    let primary_target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "127.0.0.1".to_string(),
        port: 8001,
        is_suspended: false,
    };
    let canary_target = RouteTarget {
        instance_id: InstanceId::new(),
        host: "127.0.0.1".to_string(),
        port: 8002,
        is_suspended: false,
    };
    canary_router.register("canary.mos.local", primary_target);
    canary_router.set_canary("canary.mos.local", canary_target, 10, "v2-canary");

    let t_canary = Instant::now();
    let canary_count = 10_000;
    for _ in 0..canary_count {
        let _target = canary_router.resolve("canary.mos.local").unwrap();
    }
    let canary_us = (t_canary.elapsed().as_secs_f64() * 1_000_000.0) / canary_count as f64;

    // 2.6 Live mos-edge.service (8180) sampling
    let live_client = reqwest::Client::new();
    let mut live_samples = Vec::new();
    for _ in 0..10 {
        let t_live = Instant::now();
        if let Ok(resp) = live_client.get("http://127.0.0.1:8180/health").send().await {
            if resp.status().is_success() {
                live_samples.push(t_live.elapsed().as_secs_f64() * 1000.0);
            }
        }
    }
    let stats_live = Stats::from_samples(live_samples);

    println!("  • Ingress Proxy RTT (200 reqs): Mean={:.3}ms | Min={:.3}ms | P50={:.3}ms | P95={:.3}ms | Max={:.3}ms", stats_proxy.mean, stats_proxy.min, stats_proxy.p50, stats_proxy.p95, stats_proxy.max);
    println!(
        "  • Ingress Proxy Throughput:    \x1b[1;32m{:.1} req/sec\x1b[0m (Single ephemeral thread)",
        rps
    );
    println!(
        "  • W3C Tracing Propagation:    {:.3} µs / request",
        trace_us
    );
    println!(
        "  • Canary Routing Decision:     {:.3} µs / decision",
        canary_us
    );
    if stats_live.count > 0 {
        println!(
            "  • Live mos-edge:8180 Health:   Mean={:.3}ms | P50={:.3}ms (Active systemd service)",
            stats_live.mean, stats_live.p50
        );
    }

    // =========================================================================
    // 3. TRACK C: Security, Crypto & Core Platform Micro-benchmarks
    // =========================================================================
    println!("\n--- [Track C: Crypto, RBAC, Cluster & Core Micro-benchmarks] ---");

    // 3.1 Ed25519 RBAC Signing & Verification (1,000 iterations)
    let auth_mgr = Ed25519AuthManager::new_random();
    let now = current_epoch();
    let sample_payload = RbacTokenPayload {
        token_id: "bench-tok-001".to_string(),
        tenant_id: "tenant-enterprise-benchmark".to_string(),
        role: Role::Admin,
        issued_at: now,
        expires_at: now + 3600,
    };

    let t_sign = Instant::now();
    let mut tokens = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let tok = auth_mgr.sign_token(&sample_payload).unwrap();
        tokens.push(tok);
    }
    let sign_us = (t_sign.elapsed().as_secs_f64() * 1_000_000.0) / 1000.0;

    let t_verify = Instant::now();
    for tok in &tokens {
        let _ = auth_mgr.verify_token(tok).unwrap();
    }
    let verify_us = (t_verify.elapsed().as_secs_f64() * 1_000_000.0) / 1000.0;

    // 3.2 Consistent Hash Ring (10,000 lookups with 3 nodes x 100 vnodes)
    let mut ring = ConsistentHashRing::new(100);
    ring.add_node("node-alpha-seoul");
    ring.add_node("node-beta-tokyo");
    ring.add_node("node-gamma-frankfurt");

    let t_ring = Instant::now();
    let ring_lookups = 10_000;
    for i in 0..ring_lookups {
        let key = format!("app-vibe-domain-{}.mos.local", i);
        let _node = ring.get_node(&key).unwrap();
    }
    let ring_us = (t_ring.elapsed().as_secs_f64() * 1_000_000.0) / ring_lookups as f64;

    // 3.3 eBPF XDP Rate-Limiter (10,000 packet evaluations)
    let xdp_filter = EbpfXdpFilter::new(50); // 50 req/sec limit
    let ip_normal = "192.168.1.100".parse().unwrap();
    let ip_blocked = "10.0.0.66".parse().unwrap();
    xdp_filter.block_ip(ip_blocked);

    let t_xdp = Instant::now();
    let xdp_count = 10_000;
    for i in 0..xdp_count {
        let ip = if i % 2 == 0 { ip_normal } else { ip_blocked };
        let _act = xdp_filter.evaluate_packet(ip, now);
    }
    let xdp_us = (t_xdp.elapsed().as_secs_f64() * 1_000_000.0) / xdp_count as f64;

    // 3.4 Multi-tenant Resource Quota Management (1,000 alloc/release cycles)
    let tenant_mgr = TenantManager::new();
    let tenant_id = TenantId("tenant-perf-bench".to_string());
    tenant_mgr.register_tenant(TenantNamespace::new(
        "tenant-perf-bench",
        "Perf Bench Tenant",
        100,
        102400,
        256,
    ));

    let t_quota = Instant::now();
    let quota_cycles = 1000;
    for _ in 0..quota_cycles {
        tenant_mgr.allocate(&tenant_id, 128, 1).unwrap();
        tenant_mgr.release(&tenant_id, 128, 1);
    }
    let quota_us = (t_quota.elapsed().as_secs_f64() * 1_000_000.0) / quota_cycles as f64;

    // 3.5 Real-time Metered Billing (1,000 ticks)
    let billing = BillingEngine::new(BillingRate::default());
    billing.register_account(CreditAccount::new("tenant-perf-bench", 10000.0));
    let sample_metric = UsageMetric {
        vcpu_seconds: 1.0,
        ram_gib_seconds: 0.5,
        vram_gib_seconds: 0.0,
        egress_bytes: 1024 * 1024,
    };

    let t_billing = Instant::now();
    let billing_count = 1000;
    for _ in 0..billing_count {
        let _ = billing
            .charge_usage("tenant-perf-bench", &sample_metric)
            .unwrap();
    }
    let billing_us = (t_billing.elapsed().as_secs_f64() * 1_000_000.0) / billing_count as f64;

    // 3.6 GPU Pool Scale-to-Zero Detach (1,000 cycles)
    let gpu_pool = GpuPoolManager::new();
    let gpu_dev = GpuDevice::new(0, "NVIDIA RTX 4090", "0000:01:00.0", 24576);
    gpu_pool.register_device(gpu_dev);

    let t_gpu = Instant::now();
    let gpu_cycles = 1000;
    for _ in 0..gpu_cycles {
        let inst_id = InstanceId::new();
        gpu_pool.bind_gpu_to_instance(&inst_id, 4096).unwrap();
        gpu_pool.scale_to_zero_detach(&inst_id).unwrap();
    }
    let gpu_us = (t_gpu.elapsed().as_secs_f64() * 1_000_000.0) / gpu_cycles as f64;

    println!("  • Ed25519 Token Signing:      {:.3} µs / sign", sign_us);
    println!(
        "  • Ed25519 Token Verify:       {:.3} µs / verify",
        verify_us
    );
    println!(
        "  • Consistent Hash Ring:       {:.3} µs / lookup (100 vnodes)",
        ring_us
    );
    println!(
        "  • eBPF XDP Packet Filter:     {:.3} µs / packet ({:.1}M pkt/sec equivalent)",
        xdp_us,
        1.0 / xdp_us
    );
    println!(
        "  • Multitenant Quota Alloc:    {:.3} µs / alloc+release",
        quota_us
    );
    println!(
        "  • Realtime Billing Charge:    {:.3} µs / tick",
        billing_us
    );
    println!(
        "  • GPU Scale-to-Zero Cycle:    {:.3} µs / bind+detach",
        gpu_us
    );

    println!("\n===============================================================================");
    println!("  🏆 MOS FULL-TRACK PERFORMANCE BENCHMARK COMPLETED SAFELY & SUCCESSFULLY      ");
    println!("===============================================================================");
}

fn current_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
