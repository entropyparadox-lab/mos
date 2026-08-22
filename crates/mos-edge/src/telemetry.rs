use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub trace_flags: String,
}

impl TraceContext {
    pub fn new() -> Self {
        let u1 = uuid::Uuid::new_v4().as_simple().to_string();
        let u2 = uuid::Uuid::new_v4().as_simple().to_string();
        let span_id = u2[..16].to_string();
        Self {
            trace_id: u1,
            span_id,
            trace_flags: "01".to_string(), // Sampled
        }
    }

    pub fn from_traceparent(header: &str) -> Option<Self> {
        let parts: Vec<&str> = header.split('-').collect();
        if parts.len() >= 4 && parts[0] == "00" && parts[1].len() == 32 && parts[2].len() == 16 {
            Some(Self {
                trace_id: parts[1].to_string(),
                span_id: parts[2].to_string(),
                trace_flags: parts[3].to_string(),
            })
        } else {
            None
        }
    }

    pub fn to_traceparent(&self) -> String {
        format!("00-{}-{}-{}", self.trace_id, self.span_id, self.trace_flags)
    }

    pub fn child_span(&self) -> Self {
        let u = uuid::Uuid::new_v4().as_simple().to_string();
        Self {
            trace_id: self.trace_id.clone(),
            span_id: u[..16].to_string(),
            trace_flags: self.trace_flags.clone(),
        }
    }
}

impl Default for TraceContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PipelineTraceTimer {
    pub trace: TraceContext,
    pub ingress_us: u64,
    pub routing_us: u64,
    pub wake_us: u64,
    pub guest_exec_us: u64,
}

impl PipelineTraceTimer {
    pub fn new(trace: TraceContext) -> Self {
        Self {
            trace,
            ingress_us: 0,
            routing_us: 0,
            wake_us: 0,
            guest_exec_us: 0,
        }
    }

    pub fn total_latency_us(&self) -> u64 {
        self.ingress_us + self.routing_us + self.wake_us + self.guest_exec_us
    }
}
