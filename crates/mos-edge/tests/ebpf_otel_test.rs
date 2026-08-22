use m_os_edge::{EbpfXdpFilter, PipelineTraceTimer, TraceContext, XdpAction};
use std::net::IpAddr;

#[test]
fn test_ebpf_xdp_filter_rate_limiting_and_blocking() {
    let filter = EbpfXdpFilter::new(5); // 5 requests per second
    let attacker_ip: IpAddr = "203.0.113.195".parse().unwrap();
    let safe_ip: IpAddr = "198.51.100.42".parse().unwrap();

    let now = 1787180000;

    // First 5 requests pass
    for _ in 0..5 {
        assert_eq!(filter.evaluate_packet(attacker_ip, now), XdpAction::Pass);
    }

    // 6th request is dropped by rate limiter
    assert_eq!(filter.evaluate_packet(attacker_ip, now), XdpAction::Drop);
    assert_eq!(filter.dropped_packets_count(), 1);

    // Other safe IP passes
    assert_eq!(filter.evaluate_packet(safe_ip, now), XdpAction::Pass);

    // Permanent block
    filter.block_ip(attacker_ip);
    assert_eq!(
        filter.evaluate_packet(attacker_ip, now + 1),
        XdpAction::Drop
    );
    assert_eq!(filter.dropped_packets_count(), 2);
}

#[test]
fn test_w3c_traceparent_propagation_and_timing() {
    let trace = TraceContext::new();
    let header = trace.to_traceparent();
    assert!(header.starts_with("00-"));
    assert_eq!(header.len(), 55); // 00 + 32 + 16 + 2 + 3 hyphens = 55 chars

    let parsed = TraceContext::from_traceparent(&header).expect("Failed parsing traceparent");
    assert_eq!(parsed.trace_id, trace.trace_id);
    assert_eq!(parsed.span_id, trace.span_id);

    let child = trace.child_span();
    assert_eq!(child.trace_id, trace.trace_id);
    assert_ne!(child.span_id, trace.span_id);

    let mut timer = PipelineTraceTimer::new(trace);
    timer.ingress_us = 120; // 0.12ms
    timer.routing_us = 50; // 0.05ms
    timer.wake_us = 1450; // 1.45ms
    timer.guest_exec_us = 8200; // 8.2ms

    assert_eq!(timer.total_latency_us(), 9820);
}
