use dashmap::DashMap;
use mos_core::InstanceId;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

/// Supported external stateful protocols managed by the Ambient Pooler
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatefulProtocol {
    PostgreSql,
    Redis,
    Elasticsearch,
    Kafka,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AmbientPoolConfig {
    pub protocol: StatefulProtocol,
    pub upstream_target: String,
    pub max_prewarmed_connections: usize,
    pub min_idle_connections: usize,
    pub connection_timeout_ms: u64,
    pub sanitize_session_on_release: bool, // e.g. DISCARD ALL / ROLLBACK for psql
}

impl Default for AmbientPoolConfig {
    fn default() -> Self {
        Self {
            protocol: StatefulProtocol::PostgreSql,
            upstream_target: "127.0.0.1:5432".to_string(),
            max_prewarmed_connections: 50,
            min_idle_connections: 5,
            connection_timeout_ms: 5000,
            sanitize_session_on_release: true,
        }
    }
}

/// Simulated active pre-warmed connection in the host pool
#[derive(Debug)]
pub struct AmbientConnection {
    pub connection_id: u64,
    pub protocol: StatefulProtocol,
    pub is_busy: bool,
    pub bound_instance: Option<InstanceId>,
    pub created_at: Instant,
    pub last_used_at: Instant,
    pub transaction_open: bool,
    pub total_queries_served: u64,
}

/// Host-Level Ambient Stateful Connection & Event Multiplexer
#[derive(Clone)]
pub struct AmbientPoolManager {
    config: AmbientPoolConfig,
    connections: Arc<DashMap<u64, AmbientConnection>>,
    next_conn_id: Arc<AtomicU64>,
    active_leases: Arc<AtomicUsize>,
    kafka_buffered_events: Arc<DashMap<String, Vec<Vec<u8>>>>, // topic -> buffered payloads
}

impl AmbientPoolManager {
    pub fn new(config: AmbientPoolConfig) -> Self {
        let manager = Self {
            config,
            connections: Arc::new(DashMap::new()),
            next_conn_id: Arc::new(AtomicU64::new(1)),
            active_leases: Arc::new(AtomicUsize::new(0)),
            kafka_buffered_events: Arc::new(DashMap::new()),
        };

        // Pre-warm initial idle connections
        for _ in 0..manager.config.min_idle_connections {
            manager.spawn_prewarmed_connection();
        }

        manager
    }

    fn spawn_prewarmed_connection(&self) -> u64 {
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::SeqCst);
        let conn = AmbientConnection {
            connection_id: conn_id,
            protocol: self.config.protocol,
            is_busy: false,
            bound_instance: None,
            created_at: Instant::now(),
            last_used_at: Instant::now(),
            transaction_open: false,
            total_queries_served: 0,
        };
        self.connections.insert(conn_id, conn);
        debug!(
            "🔌 Pre-warmed connection #{} established for {:?}",
            conn_id, self.config.protocol
        );
        conn_id
    }

    /// Sub-millisecond instant lease when a MicroVM wakes up (<0.05 ms)
    pub fn lease_connection_on_wake(&self, instance_id: &InstanceId) -> Result<u64, &'static str> {
        // 1. Find an existing idle pre-warmed connection
        for mut entry in self.connections.iter_mut() {
            let conn = entry.value_mut();
            if !conn.is_busy {
                conn.is_busy = true;
                conn.bound_instance = Some(*instance_id);
                conn.last_used_at = Instant::now();
                self.active_leases.fetch_add(1, Ordering::SeqCst);
                return Ok(conn.connection_id);
            }
        }

        // 2. If all busy and under max limit, spawn dynamically
        if self.connections.len() < self.config.max_prewarmed_connections {
            let conn_id = self.spawn_prewarmed_connection();
            if let Some(mut entry) = self.connections.get_mut(&conn_id) {
                let conn = entry.value_mut();
                conn.is_busy = true;
                conn.bound_instance = Some(*instance_id);
                conn.last_used_at = Instant::now();
                self.active_leases.fetch_add(1, Ordering::SeqCst);
                return Ok(conn_id);
            }
        }

        Err("Connection pool exhausted: all pre-warmed connections busy")
    }

    /// Execute a query over the leased connection
    pub fn execute_query(&self, conn_id: u64, query: &str) -> Result<usize, &'static str> {
        let mut entry = self
            .connections
            .get_mut(&conn_id)
            .ok_or("Connection not found")?;
        let conn = entry.value_mut();
        if !conn.is_busy {
            return Err("Cannot execute query on unleased connection");
        }

        conn.total_queries_served += 1;
        conn.last_used_at = Instant::now();

        if query.to_uppercase().starts_with("BEGIN") {
            conn.transaction_open = true;
        } else if query.to_uppercase().starts_with("COMMIT")
            || query.to_uppercase().starts_with("ROLLBACK")
        {
            conn.transaction_open = false;
        }

        // Returns simulated affected rows
        Ok(1)
    }

    /// Release connection back to pool on MicroVM idle/suspend with session sanitization
    pub fn release_connection_on_suspend(
        &self,
        instance_id: &InstanceId,
    ) -> Result<(), &'static str> {
        for mut entry in self.connections.iter_mut() {
            let conn = entry.value_mut();
            if conn.bound_instance == Some(*instance_id) {
                // Adversarial check: If transaction was left open, sanitize session
                if conn.transaction_open || self.config.sanitize_session_on_release {
                    // Simulates issuing "ROLLBACK; DISCARD ALL;" to prevent cross-tenant state leaks
                    conn.transaction_open = false;
                    debug!(
                        "🧹 Sanitized session (DISCARD ALL) on connection #{}",
                        conn.connection_id
                    );
                }

                conn.is_busy = false;
                conn.bound_instance = None;
                conn.last_used_at = Instant::now();
                self.active_leases.fetch_sub(1, Ordering::SeqCst);
                return Ok(());
            }
        }
        Err("No active connection bound to instance")
    }

    /// Kafka / Event Broker: Ingress buffers partition messages and wakes MicroVM via event bridge
    pub fn push_kafka_event(&self, topic: &str, payload: Vec<u8>) {
        let mut entry = self
            .kafka_buffered_events
            .entry(topic.to_string())
            .or_default();
        entry.push(payload);
    }

    /// Drain buffered events for waking MicroVM
    pub fn drain_kafka_events(&self, topic: &str) -> Vec<Vec<u8>> {
        if let Some((_, mut events)) = self.kafka_buffered_events.remove(topic) {
            std::mem::take(&mut events)
        } else {
            Vec::new()
        }
    }

    pub fn total_connections(&self) -> usize {
        self.connections.len()
    }

    pub fn active_leased_count(&self) -> usize {
        self.active_leases.load(Ordering::SeqCst)
    }
}
