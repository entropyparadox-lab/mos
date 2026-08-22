use crate::router::{EdgeRouter, RouteTarget};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryPipelineConfig {
    pub step_weights: Vec<u32>, // e.g. [10, 50, 100]
    pub min_requests_per_step: u64,
    pub max_error_rate_percent: f64, // e.g. 5.0%
}

impl Default for CanaryPipelineConfig {
    fn default() -> Self {
        Self {
            step_weights: vec![10, 50, 100],
            min_requests_per_step: 20,
            max_error_rate_percent: 5.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipelineStatus {
    Idle,
    Progressing {
        step_index: usize,
        current_weight: u32,
        version_tag: String,
    },
    Completed {
        version_tag: String,
    },
    RolledBack {
        reason: String,
        version_tag: String,
    },
}

#[derive(Debug, Default)]
pub struct StepMetrics {
    pub total_requests: AtomicU64,
    pub error_requests: AtomicU64,
}

#[derive(Clone)]
pub struct CanaryPipelineManager {
    router: EdgeRouter,
    config: CanaryPipelineConfig,
    status_map: Arc<DashMap<String, PipelineStatus>>,
    metrics_map: Arc<DashMap<String, Arc<StepMetrics>>>,
}

#[derive(Debug, PartialEq)]
pub enum PipelineEvaluation {
    ContinueMonitoring {
        current_step: usize,
        weight: u32,
        requests: u64,
        error_rate: f64,
    },
    Promoted {
        new_step: usize,
        new_weight: u32,
    },
    FullyPromoted {
        version_tag: String,
    },
    RolledBack {
        reason: String,
    },
}

impl CanaryPipelineManager {
    pub fn new(router: EdgeRouter, config: CanaryPipelineConfig) -> Self {
        Self {
            router,
            config,
            status_map: Arc::new(DashMap::new()),
            metrics_map: Arc::new(DashMap::new()),
        }
    }

    pub fn start_canary_deployment(
        &self,
        domain: &str,
        canary_target: RouteTarget,
        version_tag: impl Into<String>,
    ) {
        let tag = version_tag.into();
        let first_weight = self.config.step_weights.first().copied().unwrap_or(10);

        self.router
            .set_canary(domain, canary_target, first_weight, &tag);
        self.status_map.insert(
            domain.to_string(),
            PipelineStatus::Progressing {
                step_index: 0,
                current_weight: first_weight,
                version_tag: tag.clone(),
            },
        );
        self.metrics_map
            .insert(domain.to_string(), Arc::new(StepMetrics::default()));

        info!(
            domain = domain,
            version = %tag,
            initial_weight = first_weight,
            "Started automated Canary pipeline deployment"
        );
    }

    pub fn record_result(&self, domain: &str, is_error: bool) {
        if let Some(metrics) = self.metrics_map.get(domain) {
            metrics.total_requests.fetch_add(1, Ordering::Relaxed);
            if is_error {
                metrics.error_requests.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn evaluate_and_advance(&self, domain: &str) -> PipelineEvaluation {
        let current_status = match self.status_map.get(domain) {
            Some(s) => s.clone(),
            None => {
                return PipelineEvaluation::RolledBack {
                    reason: "No active canary pipeline for domain".to_string(),
                }
            }
        };

        match current_status {
            PipelineStatus::Progressing {
                step_index,
                current_weight,
                version_tag,
            } => {
                let metrics = match self.metrics_map.get(domain) {
                    Some(m) => m.clone(),
                    None => {
                        return PipelineEvaluation::RolledBack {
                            reason: "Metrics missing".to_string(),
                        }
                    }
                };

                let total = metrics.total_requests.load(Ordering::Relaxed);
                let errors = metrics.error_requests.load(Ordering::Relaxed);

                let error_rate = if total > 0 {
                    (errors as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                // Check error threshold -> Automatic Rollback
                if error_rate > self.config.max_error_rate_percent && total >= 5 {
                    let reason = format!(
                        "Canary error rate {:.2}% exceeded threshold {:.2}% (errors: {}/{})",
                        error_rate, self.config.max_error_rate_percent, errors, total
                    );
                    warn!(domain = domain, reason = %reason, "Triggering automatic canary rollback");
                    self.router.rollback_canary(domain);
                    self.status_map.insert(
                        domain.to_string(),
                        PipelineStatus::RolledBack {
                            reason: reason.clone(),
                            version_tag,
                        },
                    );
                    return PipelineEvaluation::RolledBack { reason };
                }

                // Check minimum requests for promotion
                if total >= self.config.min_requests_per_step {
                    let next_step_index = step_index + 1;
                    if next_step_index < self.config.step_weights.len()
                        && self.config.step_weights[next_step_index] < 100
                    {
                        let next_weight = self.config.step_weights[next_step_index];
                        self.router.promote_canary_step(domain, next_weight);

                        // Reset metrics for next step
                        metrics.total_requests.store(0, Ordering::Relaxed);
                        metrics.error_requests.store(0, Ordering::Relaxed);

                        self.status_map.insert(
                            domain.to_string(),
                            PipelineStatus::Progressing {
                                step_index: next_step_index,
                                current_weight: next_weight,
                                version_tag,
                            },
                        );

                        info!(
                            domain = domain,
                            step = next_step_index,
                            weight = next_weight,
                            "Promoted canary to next stage"
                        );

                        PipelineEvaluation::Promoted {
                            new_step: next_step_index,
                            new_weight: next_weight,
                        }
                    } else {
                        // Final step: 100% full promotion
                        self.router.promote_canary_step(domain, 100);
                        self.status_map.insert(
                            domain.to_string(),
                            PipelineStatus::Completed {
                                version_tag: version_tag.clone(),
                            },
                        );
                        info!(
                            domain = domain,
                            version = %version_tag,
                            "Canary pipeline reached 100% full production promotion"
                        );
                        PipelineEvaluation::FullyPromoted { version_tag }
                    }
                } else {
                    PipelineEvaluation::ContinueMonitoring {
                        current_step: step_index,
                        weight: current_weight,
                        requests: total,
                        error_rate,
                    }
                }
            }
            PipelineStatus::Completed { version_tag } => {
                PipelineEvaluation::FullyPromoted { version_tag }
            }
            PipelineStatus::RolledBack { reason, .. } => PipelineEvaluation::RolledBack { reason },
            PipelineStatus::Idle => PipelineEvaluation::RolledBack {
                reason: "Idle".to_string(),
            },
        }
    }

    pub fn get_pipeline_status(&self, domain: &str) -> PipelineStatus {
        self.status_map
            .get(domain)
            .map(|r| r.clone())
            .unwrap_or(PipelineStatus::Idle)
    }
}
