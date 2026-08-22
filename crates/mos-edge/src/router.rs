use dashmap::DashMap;
use mos_core::InstanceId;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RouteTarget {
    pub instance_id: InstanceId,
    pub host: String,
    pub port: u16,
    pub is_suspended: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigRouteEntry {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub is_suspended: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RoutingConfigFile {
    #[serde(default)]
    pub routes: std::collections::HashMap<String, ConfigRouteEntry>,
}

#[derive(Debug, Clone)]
pub struct WeightedTarget {
    pub target: RouteTarget,
    pub weight: u32, // 1 ~ 100
    pub version_tag: String,
}

#[derive(Debug, Clone)]
pub struct DomainRoutingEntry {
    pub stable: WeightedTarget,
    pub canary: Option<WeightedTarget>,
}

#[derive(Clone, Default)]
pub struct EdgeRouter {
    routes: Arc<DashMap<String, DomainRoutingEntry>>,
    counter: Arc<AtomicU64>,
}

impl EdgeRouter {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(DashMap::new()),
            counter: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn register(&self, domain: impl Into<String>, target: RouteTarget) {
        let domain_str = domain.into();
        let entry = DomainRoutingEntry {
            stable: WeightedTarget {
                target,
                weight: 100,
                version_tag: "v1-stable".to_string(),
            },
            canary: None,
        };
        self.routes.insert(domain_str, entry);
    }

    pub fn set_canary(
        &self,
        domain: &str,
        canary_target: RouteTarget,
        canary_weight: u32,
        version_tag: impl Into<String>,
    ) {
        let weight = canary_weight.min(100);
        let stable_weight = 100 - weight;

        if let Some(mut entry) = self.routes.get_mut(domain) {
            entry.stable.weight = stable_weight;
            entry.canary = Some(WeightedTarget {
                target: canary_target,
                weight,
                version_tag: version_tag.into(),
            });
            info!(
                domain = domain,
                canary_weight = weight,
                stable_weight = stable_weight,
                "Updated canary weight"
            );
        }
    }

    pub fn promote_canary_step(&self, domain: &str, new_canary_weight: u32) {
        if new_canary_weight >= 100 {
            // Full promotion to stable
            if let Some(mut entry) = self.routes.get_mut(domain) {
                if let Some(canary) = entry.canary.take() {
                    info!(domain = domain, version = %canary.version_tag, "Canary fully promoted to stable 100%");
                    entry.stable = WeightedTarget {
                        target: canary.target,
                        weight: 100,
                        version_tag: canary.version_tag,
                    };
                }
            }
        } else {
            if let Some(mut entry) = self.routes.get_mut(domain) {
                if let Some(canary) = &mut entry.canary {
                    canary.weight = new_canary_weight;
                    entry.stable.weight = 100 - new_canary_weight;
                    info!(
                        domain = domain,
                        canary_weight = new_canary_weight,
                        "Canary weight promoted step"
                    );
                }
            }
        }
    }

    pub fn rollback_canary(&self, domain: &str) {
        if let Some(mut entry) = self.routes.get_mut(domain) {
            if let Some(removed) = entry.canary.take() {
                entry.stable.weight = 100;
                info!(domain = domain, discarded_version = %removed.version_tag, "Canary rolled back to stable 100%");
            }
        }
    }

    pub fn resolve(&self, domain: &str) -> Option<RouteTarget> {
        let entry = self.routes.get(domain)?;
        if let Some(canary) = &entry.canary {
            if canary.weight > 0 {
                let req_count = self.counter.fetch_add(1, Ordering::Relaxed);
                let bucket = (req_count % 100) as u32; // 0..99
                if bucket < canary.weight {
                    debug!(
                        domain = domain,
                        "Routed to Canary (weight: {}%)", canary.weight
                    );
                    return Some(canary.target.clone());
                }
            }
        }
        Some(entry.stable.target.clone())
    }

    pub fn inspect_routes(&self, domain: &str) -> Option<DomainRoutingEntry> {
        self.routes.get(domain).map(|r| r.value().clone())
    }

    pub fn load_from_json(&self, json_content: &str) -> anyhow::Result<usize> {
        let config: RoutingConfigFile = serde_json::from_str(json_content)?;
        let count = config.routes.len();
        for (domain, entry) in config.routes {
            let target = RouteTarget {
                instance_id: InstanceId::new(),
                host: entry.host,
                port: entry.port,
                is_suspended: entry.is_suspended,
            };
            self.register(domain, target);
        }
        Ok(count)
    }

    pub fn list_domains(&self) -> Vec<String> {
        self.routes.iter().map(|kv| kv.key().clone()).collect()
    }
}
