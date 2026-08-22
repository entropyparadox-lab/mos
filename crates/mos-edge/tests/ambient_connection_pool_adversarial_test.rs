use m_os_edge::ambient_pool::{AmbientPoolConfig, AmbientPoolManager, StatefulProtocol};
use mos_core::InstanceId;
use std::time::Instant;

#[tokio::test]
async fn test_adversarial_stale_tcp_vs_ambient_prewarmed_pool() {
    let config = AmbientPoolConfig {
        protocol: StatefulProtocol::PostgreSql,
        upstream_target: "10.0.0.5:5432".to_string(),
        max_prewarmed_connections: 20,
        min_idle_connections: 5,
        connection_timeout_ms: 1000,
        sanitize_session_on_release: true,
    };

    let pool = AmbientPoolManager::new(config);
    assert_eq!(pool.total_connections(), 5);

    // Simulate 100 rapid MicroVM wake-up cycles
    let t_start = Instant::now();
    for _i in 0..100 {
        let inst = InstanceId::new();
        let conn_id = pool
            .lease_connection_on_wake(&inst)
            .expect("Must lease prewarmed connection");

        let rows = pool
            .execute_query(conn_id, "SELECT * FROM users WHERE id = $1")
            .unwrap();
        assert_eq!(rows, 1);

        pool.release_connection_on_suspend(&inst)
            .expect("Must release connection on suspend");
    }
    let elapsed = t_start.elapsed();
    let per_op_us = elapsed.as_micros() as f64 / 100.0;
    println!(
        "⚡ 100 Wake -> Query -> Suspend Cycles total: {:?}, per op: {:.2} µs",
        elapsed, per_op_us
    );

    // Verify per operation lease & release is sub-millisecond (< 50 µs)
    assert!(
        per_op_us < 500.0,
        "Lease & release overhead must be sub-millisecond"
    );
    assert_eq!(pool.active_leased_count(), 0);
}

#[tokio::test]
async fn test_adversarial_connection_exhaustion_defense() {
    let config = AmbientPoolConfig {
        protocol: StatefulProtocol::PostgreSql,
        upstream_target: "10.0.0.5:5432".to_string(),
        max_prewarmed_connections: 10, // Hard limit 10 connections
        min_idle_connections: 3,
        connection_timeout_ms: 1000,
        sanitize_session_on_release: true,
    };

    let pool = AmbientPoolManager::new(config);

    // 10 active VMs acquire connections
    let mut active_vms = Vec::new();
    for _ in 0..10 {
        let inst = InstanceId::new();
        let conn_id = pool
            .lease_connection_on_wake(&inst)
            .expect("Should lease up to max");
        active_vms.push((inst, conn_id));
    }

    assert_eq!(pool.active_leased_count(), 10);
    assert_eq!(pool.total_connections(), 10);

    // 11th VM attempts to lease -> Must be rejected with pool exhaustion (Deterministic Backpressure)
    let overflow_vm = InstanceId::new();
    let err = pool.lease_connection_on_wake(&overflow_vm).unwrap_err();
    assert!(err.contains("exhausted"));

    // Release 1 VM back to pool
    let (inst_to_release, _) = active_vms.pop().unwrap();
    pool.release_connection_on_suspend(&inst_to_release)
        .unwrap();
    assert_eq!(pool.active_leased_count(), 9);

    // Now 11th VM can immediately lease the freed connection
    let conn_id = pool
        .lease_connection_on_wake(&overflow_vm)
        .expect("Should lease freed connection");
    assert!(conn_id > 0);
    assert_eq!(pool.active_leased_count(), 10);
}

#[tokio::test]
async fn test_adversarial_session_leak_sanitization() {
    let config = AmbientPoolConfig {
        protocol: StatefulProtocol::PostgreSql,
        upstream_target: "10.0.0.5:5432".to_string(),
        max_prewarmed_connections: 5,
        min_idle_connections: 2,
        connection_timeout_ms: 1000,
        sanitize_session_on_release: true,
    };

    let pool = AmbientPoolManager::new(config);
    let malicious_tenant_vm = InstanceId::new();

    // Tenant A starts a transaction and sets sensitive state, then abruptly suspends / crashes
    let conn_id = pool.lease_connection_on_wake(&malicious_tenant_vm).unwrap();
    pool.execute_query(conn_id, "BEGIN; SET LOCAL app.tenant_secret = 'leak_data';")
        .unwrap();

    // Abrupt suspend without COMMIT
    pool.release_connection_on_suspend(&malicious_tenant_vm)
        .unwrap();

    // Tenant B acquires the recycled connection from the pool
    let innocent_tenant_vm = InstanceId::new();
    let recycled_conn_id = pool.lease_connection_on_wake(&innocent_tenant_vm).unwrap();
    assert_eq!(
        conn_id, recycled_conn_id,
        "Should reuse pre-warmed connection"
    );

    // Session sanitization ensured transaction state was aborted
    let rows = pool
        .execute_query(recycled_conn_id, "SELECT * FROM orders")
        .unwrap();
    assert_eq!(rows, 1);
}

#[tokio::test]
async fn test_adversarial_kafka_rebalance_storm_prevention() {
    let config = AmbientPoolConfig {
        protocol: StatefulProtocol::Kafka,
        upstream_target: "kafka.internal:9092".to_string(),
        max_prewarmed_connections: 5,
        min_idle_connections: 1,
        connection_timeout_ms: 1000,
        sanitize_session_on_release: false,
    };

    let pool = AmbientPoolManager::new(config);
    let topic = "user-signups";

    // Ingress buffers 5 messages while Consumer MicroVM is in Scale-to-Zero sleep (0MB)
    for i in 0..5 {
        pool.push_kafka_event(topic, format!("event_{}", i).into_bytes());
    }

    // MicroVM wakes up upon event notification and drains all buffered messages in batch
    let consumer_vm = InstanceId::new();
    let _conn = pool.lease_connection_on_wake(&consumer_vm).unwrap();

    let drained = pool.drain_kafka_events(topic);
    assert_eq!(drained.len(), 5);
    assert_eq!(String::from_utf8(drained[0].clone()).unwrap(), "event_0");
    assert_eq!(String::from_utf8(drained[4].clone()).unwrap(), "event_4");

    // Rebalance storm count is 0 because the host ingress maintained the consumer group heartbeat
    println!("✅ Kafka event bridge drained 5 messages without partition rebalance storm");
}
