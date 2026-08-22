use anyhow::Result;
use nix::sys::signal::{kill, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub enum ProcessEvent {
    Started { pid: u32 },
    StdoutLine(String),
    StderrLine(String),
    Exited { code: Option<i32> },
}

pub struct AppSupervisor {
    child_pid: Option<u32>,
    running: Arc<AtomicBool>,
}

impl Default for AppSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl AppSupervisor {
    pub fn new() -> Self {
        Self {
            child_pid: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child_pid
    }

    pub async fn spawn(
        &mut self,
        cmd: &str,
        args: &[String],
        env: &HashMap<String, String>,
        event_tx: mpsc::Sender<ProcessEvent>,
    ) -> Result<Child> {
        info!(
            "▶️ [mos-init supervisor] Spawning application: {} {:?}",
            cmd, args
        );

        let mut command = Command::new(cmd);
        command.args(args);
        for (k, v) in env {
            command.env(k, v);
        }
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command.spawn()?;
        let pid = child.id().unwrap_or(0);
        self.child_pid = Some(pid);
        self.running.store(true, Ordering::SeqCst);

        let _ = event_tx.send(ProcessEvent::Started { pid }).await;

        // Pipe stdout
        if let Some(stdout) = child.stdout.take() {
            let tx = event_tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = tx.send(ProcessEvent::StdoutLine(line)).await;
                }
            });
        }

        // Pipe stderr
        if let Some(stderr) = child.stderr.take() {
            let tx = event_tx.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    let _ = tx.send(ProcessEvent::StderrLine(line)).await;
                }
            });
        }

        Ok(child)
    }

    pub fn forward_signal(&self, signal: Signal) -> Result<()> {
        if let Some(pid) = self.child_pid {
            let target_pid = Pid::from_raw(pid as i32);
            info!("Forwarding signal {:?} to child PID {}", signal, pid);
            kill(target_pid, signal)?;
        }
        Ok(())
    }

    pub fn reap_zombies() -> Vec<(i32, Option<i32>)> {
        let mut reaped = Vec::new();
        loop {
            match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(pid, code)) => {
                    debug!("Reaped zombie process PID {} (exit code {})", pid, code);
                    reaped.push((pid.as_raw(), Some(code)));
                }
                Ok(WaitStatus::Signaled(pid, sig, _)) => {
                    debug!("Reaped zombie process PID {} (signal {:?})", pid, sig);
                    reaped.push((pid.as_raw(), None));
                }
                Ok(WaitStatus::StillAlive) => break,
                Err(nix::errno::Errno::ECHILD) => break, // No more child processes
                Err(e) => {
                    warn!("Error during zombie reaping: {}", e);
                    break;
                }
                _ => break,
            }
        }
        reaped
    }
}
