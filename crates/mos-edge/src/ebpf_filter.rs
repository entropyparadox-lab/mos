use dashmap::DashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, warn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XdpAction {
    Pass,
    Drop,
    RateLimit,
}

#[derive(Clone)]
pub struct EbpfXdpFilter {
    blocked_ips: Arc<DashMap<IpAddr, u64>>, // IP -> Block expiry epoch
    request_rates: Arc<DashMap<IpAddr, (u64, u32)>>, // IP -> (window_start_sec, req_count)
    rate_limit_per_sec: u32,
    dropped_counter: Arc<AtomicU64>,
}

impl EbpfXdpFilter {
    pub fn new(rate_limit_per_sec: u32) -> Self {
        Self {
            blocked_ips: Arc::new(DashMap::new()),
            request_rates: Arc::new(DashMap::new()),
            rate_limit_per_sec,
            dropped_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn block_ip(&self, ip: IpAddr) {
        self.blocked_ips.insert(ip, u64::MAX);
        warn!(ip = %ip, "eBPF XDP: IP permanently blocked");
    }

    pub fn unblock_ip(&self, ip: &IpAddr) {
        self.blocked_ips.remove(ip);
    }

    pub fn evaluate_packet(&self, src_ip: IpAddr, current_sec: u64) -> XdpAction {
        // 1. Hard blocklist check (XDP_DROP)
        if self.blocked_ips.contains_key(&src_ip) {
            self.dropped_counter.fetch_add(1, Ordering::Relaxed);
            return XdpAction::Drop;
        }

        // 2. Sliding window rate limiter (XDP_PASS or XDP_DROP)
        let mut entry = self.request_rates.entry(src_ip).or_insert((current_sec, 0));
        let (window_start, count) = entry.value_mut();

        if *window_start == current_sec {
            *count += 1;
            if *count > self.rate_limit_per_sec {
                self.dropped_counter.fetch_add(1, Ordering::Relaxed);
                debug!(ip = %src_ip, "eBPF XDP: Rate limit exceeded (XDP_DROP)");
                return XdpAction::Drop;
            }
        } else {
            *window_start = current_sec;
            *count = 1;
        }

        XdpAction::Pass
    }

    pub fn dropped_packets_count(&self) -> u64 {
        self.dropped_counter.load(Ordering::Relaxed)
    }
}
