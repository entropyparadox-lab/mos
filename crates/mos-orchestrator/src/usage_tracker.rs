use dashmap::DashMap;
use mos_core::{BillingEngine, BillingError, InstanceId, UsageMetric};
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

#[derive(Debug, Clone)]
pub struct InstanceResourceProfile {
    pub instance_id: InstanceId,
    pub tenant_id: String,
    pub vcpu_count: u32,
    pub ram_mib: u64,
    pub vram_mib: u64,
    pub egress_bytes: u64,
    pub started_at: Instant,
    pub last_tick: Instant,
}

#[derive(Clone)]
pub struct UsageTracker {
    active_instances: Arc<DashMap<InstanceId, InstanceResourceProfile>>,
    billing_engine: BillingEngine,
    accumulated_tenant_metrics: Arc<DashMap<String, UsageMetric>>,
}

impl UsageTracker {
    pub fn new(billing_engine: BillingEngine) -> Self {
        Self {
            active_instances: Arc::new(DashMap::new()),
            billing_engine,
            accumulated_tenant_metrics: Arc::new(DashMap::new()),
        }
    }

    pub fn start_tracking(
        &self,
        instance_id: InstanceId,
        tenant_id: impl Into<String>,
        vcpu_count: u32,
        ram_mib: u64,
        vram_mib: u64,
    ) {
        let now = Instant::now();
        let profile = InstanceResourceProfile {
            instance_id,
            tenant_id: tenant_id.into(),
            vcpu_count,
            ram_mib,
            vram_mib,
            egress_bytes: 0,
            started_at: now,
            last_tick: now,
        };
        self.active_instances.insert(instance_id, profile);
    }

    pub fn record_egress(&self, instance_id: &InstanceId, bytes: u64) {
        if let Some(mut profile) = self.active_instances.get_mut(instance_id) {
            profile.egress_bytes += bytes;
        }
    }

    pub fn tick_and_charge(&self) -> Vec<(String, Result<f64, BillingError>)> {
        let now = Instant::now();
        let mut results = Vec::new();

        for mut profile in self.active_instances.iter_mut() {
            let elapsed_secs = profile.last_tick.elapsed().as_secs_f64();
            profile.last_tick = now;

            let vcpu_secs = (profile.vcpu_count as f64) * elapsed_secs;
            let ram_gib_secs = (profile.ram_mib as f64 / 1024.0) * elapsed_secs;
            let vram_gib_secs = (profile.vram_mib as f64 / 1024.0) * elapsed_secs;
            let egress_chunk = profile.egress_bytes;
            profile.egress_bytes = 0; // reset delta

            let metric = UsageMetric {
                vcpu_seconds: vcpu_secs,
                ram_gib_seconds: ram_gib_secs,
                vram_gib_seconds: vram_gib_secs,
                egress_bytes: egress_chunk,
            };

            // Accumulate
            let mut acc = self
                .accumulated_tenant_metrics
                .entry(profile.tenant_id.clone())
                .or_default();
            acc.add(&metric);

            // Charge
            let res = self
                .billing_engine
                .charge_usage(&profile.tenant_id, &metric);
            results.push((profile.tenant_id.clone(), res));
        }

        results
    }

    pub fn stop_tracking(&self, instance_id: &InstanceId) -> Option<InstanceResourceProfile> {
        let removed = self.active_instances.remove(instance_id).map(|(_, v)| v);
        if let Some(ref p) = removed {
            info!(instance = %instance_id, tenant = %p.tenant_id, "Stopped tracking instance usage");
        }
        removed
    }

    pub fn get_tenant_metric(&self, tenant_id: &str) -> UsageMetric {
        self.accumulated_tenant_metrics
            .get(tenant_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }
}
