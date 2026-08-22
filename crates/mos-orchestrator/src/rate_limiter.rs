use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenBucket {
    pub size: u64,
    pub one_time_burst: Option<u64>,
    pub refill_time: u64, // milliseconds
}

impl TokenBucket {
    pub fn new(size: u64, refill_time_ms: u64) -> Self {
        Self {
            size,
            one_time_burst: Some(size * 2),
            refill_time: refill_time_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RateLimiterConfig {
    pub bandwidth: Option<TokenBucket>,
    pub ops: Option<TokenBucket>,
}

impl RateLimiterConfig {
    pub fn network_default() -> Self {
        Self {
            // 100 Mbps (12.5 MB/s)
            bandwidth: Some(TokenBucket::new(12_500_000, 1000)),
            // 10,000 packets per second
            ops: Some(TokenBucket::new(10_000, 1000)),
        }
    }

    pub fn disk_default() -> Self {
        Self {
            // 50 MB/s disk I/O limit
            bandwidth: Some(TokenBucket::new(50_000_000, 1000)),
            // 2,000 IOPS
            ops: Some(TokenBucket::new(2_000, 1000)),
        }
    }
}
