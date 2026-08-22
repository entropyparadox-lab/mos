use mos_cluster::{ClusterNode, GlobalIngressRouter, GlobalRouteDecision, NodeStatus};
use mos_edge::{
    ebpf_filter::{EbpfXdpFilter, XdpAction},
    telemetry::{PipelineTraceTimer, TraceContext},
    EdgeRouter,
};
use mos_orchestrator::{SnapshotArtifacts, UffdSnapshotEngine};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;

#[test]
fn test_phase6_multi_node_cluster_and_global_routing() {
    let local_addr: SocketAddr = "10.0.1.10:8080".parse().unwrap();
    let edge_router = EdgeRouter::new();
    let global_ingress = GlobalIngressRouter::new("node-kr-seoul-1", local_addr, edge_router);

    // 1. Register 2 remote nodes in P2P Gossip
    let node_us = ClusterNode {
        node_id: "node-us-east-1".to_string(),
        addr: "10.0.2.10:8080".parse().unwrap(),
        status: NodeStatus::Alive,
        last_seen_epoch: 1787180000,
        capacity_vms: 1000,
        active_vms: 45,
    };
    let node_eu = ClusterNode {
        node_id: "node-eu-central-1".to_string(),
        addr: "10.0.3.10:8080".parse().unwrap(),
        status: NodeStatus::Alive,
        last_seen_epoch: 1787180000,
        capacity_vms: 1000,
        active_vms: 30,
    };

    global_ingress.membership.register_or_update(node_us);
    global_ingress.membership.register_or_update(node_eu);
    global_ingress.sync_ring_from_alive_nodes();

    // 2. Route resolution across cluster
    let domains = [
        "vibe-app-1.mos.dev",
        "vibe-app-2.mos.dev",
        "vibe-app-3.mos.dev",
        "vibe-app-4.mos.dev",
    ];

    for domain in domains {
        let decision = global_ingress.route_decision(domain);
        match decision {
            GlobalRouteDecision::ServeLocal => {
                assert_eq!(global_ingress.local_node_id, "node-kr-seoul-1");
            }
            GlobalRouteDecision::ForwardToNode {
                node_id,
                target_addr,
            } => {
                assert!(node_id == "node-us-east-1" || node_id == "node-eu-central-1");
                assert!(target_addr.port() == 8080);
            }
            GlobalRouteDecision::ClusterEmpty => panic!("Cluster must not be empty"),
        }
    }
}

#[test]
fn test_phase6_uffd_zstd_compression_benchmark() {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let out_dir = temp_dir.path().join("compressed_snaps");
    let decompress_dir = temp_dir.path().join("decompressed_snaps");
    fs::create_dir_all(&decompress_dir).unwrap();

    let snap_path = temp_dir.path().join("inst-01.snap");
    let mem_path = temp_dir.path().join("inst-01.mem");
    let rootfs_path = temp_dir.path().join("rootfs.ext4");

    fs::write(&snap_path, "FIRECRACKER_HEADER_V1").unwrap();
    fs::write(&rootfs_path, "EXT4_IMAGE").unwrap();

    // Simulated 512KB sparse memory
    let repeated_block = vec![0xA5u8; 8192];
    let mut full_mem = Vec::new();
    for _ in 0..64 {
        full_mem.extend_from_slice(&repeated_block);
    }
    fs::write(&mem_path, &full_mem).unwrap();

    let artifacts = SnapshotArtifacts {
        snapshot_path: snap_path,
        mem_path,
        rootfs_path,
    };

    let engine = UffdSnapshotEngine::new(PathBuf::from("/usr/bin/firecracker"));
    let compressed = engine.compress_snapshot(&artifacts, &out_dir).unwrap();

    assert!(compressed.compressed_size_bytes < compressed.original_size_bytes);
    assert!(compressed.compression_ratio < 0.05); // >95% compression on sparse memory

    let restored_mem = decompress_dir.join("inst-01.restored.mem");
    let _restored = engine
        .decompress_snapshot(&compressed, &restored_mem)
        .unwrap();

    assert_eq!(fs::read(&restored_mem).unwrap(), full_mem);
}

#[test]
fn test_phase6_ebpf_and_otel_distributed_tracing() {
    // 1. eBPF packet filter
    let filter = EbpfXdpFilter::new(10);
    let src_ip = "192.0.2.1".parse().unwrap();
    let current_sec = 1787180000;

    for _ in 0..10 {
        assert_eq!(filter.evaluate_packet(src_ip, current_sec), XdpAction::Pass);
    }
    assert_eq!(filter.evaluate_packet(src_ip, current_sec), XdpAction::Drop);

    // 2. OpenTelemetry W3C distributed trace context
    let trace = TraceContext::new();
    let header = trace.to_traceparent();
    let parsed_ctx = TraceContext::from_traceparent(&header).unwrap();
    assert_eq!(parsed_ctx.trace_id, trace.trace_id);

    let mut timer = PipelineTraceTimer::new(parsed_ctx);
    timer.ingress_us = 95; // 0.095ms
    timer.routing_us = 45; // 0.045ms (Consistent hash lookup)
    timer.wake_us = 1200; // 1.200ms (UFFD on-demand resume)
    timer.guest_exec_us = 5500; // 5.5ms

    assert!(timer.total_latency_us() < 7000); // < 7ms total end-to-end
}
