use m_os_init::{
    mount::mount_early_filesystems,
    network::configure_networking,
    supervisor::{AppSupervisor, ProcessEvent},
    vsock_ipc::{get_process_rss_bytes, GuestMessage},
};
use nix::sys::signal::Signal;
use std::collections::HashMap;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_mount_and_network_init_safety() {
    // Should run safely without crashing even in unprivileged test environment
    let mount_res = mount_early_filesystems();
    assert!(mount_res.is_ok());

    let net_res = configure_networking(Some("172.16.0.2"), Some("172.16.0.1"));
    assert!(net_res.is_ok());
}

#[tokio::test]
async fn test_supervisor_lifecycle_and_output_capture() {
    let (tx, mut rx) = mpsc::channel(50);
    let mut supervisor = AppSupervisor::new();
    let mut env = HashMap::new();
    env.insert("TEST_VAR".to_string(), "MOS_VAL".to_string());

    let mut child = supervisor
        .spawn(
            "sh",
            &[
                "-c".to_string(),
                "echo $TEST_VAR; echo 'error stream' >&2".to_string(),
            ],
            &env,
            tx,
        )
        .await
        .expect("Failed to spawn process");

    let mut stdout_captured = Vec::new();
    let mut stderr_captured = Vec::new();

    while let Some(event) = rx.recv().await {
        match event {
            ProcessEvent::Started { pid } => {
                assert!(pid > 0);
            }
            ProcessEvent::StdoutLine(line) => {
                stdout_captured.push(line);
            }
            ProcessEvent::StderrLine(line) => {
                stderr_captured.push(line);
            }
            ProcessEvent::Exited { .. } => break,
        }
        if !stdout_captured.is_empty() && !stderr_captured.is_empty() {
            break;
        }
    }

    let _ = child.wait().await;

    assert!(stdout_captured.iter().any(|l| l.contains("MOS_VAL")));
    assert!(stderr_captured.iter().any(|l| l.contains("error stream")));
}

#[tokio::test]
async fn test_supervisor_signal_and_zombie_reaping() {
    let (tx, _rx) = mpsc::channel(50);
    let mut supervisor = AppSupervisor::new();
    let env = HashMap::new();

    let mut child = supervisor
        .spawn("sleep", &["10".to_string()], &env, tx)
        .await
        .expect("Failed to spawn sleep");

    assert!(supervisor.child_pid().is_some());

    // Forward SIGTERM
    let sig_res = supervisor.forward_signal(Signal::SIGTERM);
    assert!(sig_res.is_ok());

    let status = child.wait().await.expect("Failed waiting child");
    assert!(!status.success()); // Terminated by signal

    // Reaping zombies test
    let _reaped = AppSupervisor::reap_zombies();
}

#[tokio::test]
async fn test_vsock_message_serialization() {
    let hb = GuestMessage::Heartbeat {
        status: "running".to_string(),
        memory_rss_bytes: 18 * 1024 * 1024,
        uptime_secs: 42,
    };
    let json = serde_json::to_string(&hb).expect("Failed serialization");
    assert!(json.contains("heartbeat"));
    assert!(json.contains("18874368"));

    let log_msg = GuestMessage::Log {
        timestamp: 1787180000,
        is_stderr: false,
        line: "Server listening on 0.0.0.0:8080".to_string(),
    };
    let json_log = serde_json::to_string(&log_msg).expect("Failed serialization");
    assert!(json_log.contains("listening on 0.0.0.0:8080"));

    let rss = get_process_rss_bytes();
    assert!(rss > 0);
}
