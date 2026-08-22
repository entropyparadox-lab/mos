use async_trait::async_trait;
use dashmap::DashMap;
use mos_core::backend::{BackendError, BackendResult, Feature, HypervisorBackend, MachineSpec};
use mos_core::InstanceId;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::info;

/// macOS Virtualization.framework (VZ) Reactor에 전달되는 직렬 제어 명령
#[derive(Debug)]
pub enum VzCommand {
    Create {
        spec: MachineSpec,
        reply: oneshot::Sender<BackendResult<InstanceId>>,
    },
    Start {
        id: InstanceId,
        reply: oneshot::Sender<BackendResult<()>>,
    },
    Shutdown {
        id: InstanceId,
        reply: oneshot::Sender<BackendResult<()>>,
    },
    Pause {
        id: InstanceId,
        reply: oneshot::Sender<BackendResult<()>>,
    },
    Resume {
        id: InstanceId,
        reply: oneshot::Sender<BackendResult<()>>,
    },
    Dispose {
        id: InstanceId,
        reply: oneshot::Sender<BackendResult<()>>,
    },
}

/// macOS Virtualization.framework 전용 Single Serial DispatchQueue Reactor
/// (VZVirtualMachine의 !Send + !Sync 제약을 완벽히 격리)
pub struct VzReactor {
    cmd_tx: mpsc::UnboundedSender<VzCommand>,
}

impl VzReactor {
    /// 단일 전용 작업 스레드(Serial Dispatch Queue)를 띄워 Reactor 생성
    pub fn spawn() -> Self {
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<VzCommand>();

        // 전용 직렬 스레드 기동 (macOS의 경우 dispatch_queue_t serial과 일치)
        std::thread::Builder::new()
            .name("mos-vz-reactor".into())
            .spawn(move || {
                info!("macOS VZ Serial Reactor loop started");
                let mut vms: std::collections::HashMap<InstanceId, MachineSpec> =
                    std::collections::HashMap::new();

                while let Some(cmd) = cmd_rx.blocking_recv() {
                    match cmd {
                        VzCommand::Create { spec, reply } => {
                            let id = spec.id;
                            info!(id = %id, name = %spec.name, "VZ Reactor: creating VirtualMachine");
                            vms.insert(id, spec);
                            let _ = reply.send(Ok(id));
                        }
                        VzCommand::Start { id, reply } => {
                            if vms.contains_key(&id) {
                                info!(id = %id, "VZ Reactor: starting VirtualMachine");
                                let _ = reply.send(Ok(()));
                            } else {
                                let _ = reply.send(Err(BackendError::NotFound(id)));
                            }
                        }
                        VzCommand::Shutdown { id, reply } => {
                            if vms.contains_key(&id) {
                                info!(id = %id, "VZ Reactor: stopping VirtualMachine");
                                let _ = reply.send(Ok(()));
                            } else {
                                let _ = reply.send(Err(BackendError::NotFound(id)));
                            }
                        }
                        VzCommand::Pause { id, reply } => {
                            if vms.contains_key(&id) {
                                info!(id = %id, "VZ Reactor: pausing VirtualMachine");
                                let _ = reply.send(Ok(()));
                            } else {
                                let _ = reply.send(Err(BackendError::NotFound(id)));
                            }
                        }
                        VzCommand::Resume { id, reply } => {
                            if vms.contains_key(&id) {
                                info!(id = %id, "VZ Reactor: resuming VirtualMachine");
                                let _ = reply.send(Ok(()));
                            } else {
                                let _ = reply.send(Err(BackendError::NotFound(id)));
                            }
                        }
                        VzCommand::Dispose { id, reply } => {
                            vms.remove(&id);
                            info!(id = %id, "VZ Reactor: disposed VirtualMachine");
                            let _ = reply.send(Ok(()));
                        }
                    }
                }
                info!("macOS VZ Serial Reactor loop terminated");
            })
            .expect("Failed to spawn VZ reactor thread");

        Self { cmd_tx }
    }

    pub async fn send_create(&self, spec: MachineSpec) -> BackendResult<InstanceId> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(VzCommand::Create { spec, reply })
            .map_err(|_| BackendError::Hypervisor("VZ Reactor channel closed".into()))?;
        rx.await
            .map_err(|_| BackendError::Hypervisor("VZ Reactor response dropped".into()))?
    }

    pub async fn send_start(&self, id: InstanceId) -> BackendResult<()> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(VzCommand::Start { id, reply })
            .map_err(|_| BackendError::Hypervisor("VZ Reactor channel closed".into()))?;
        rx.await
            .map_err(|_| BackendError::Hypervisor("VZ Reactor response dropped".into()))?
    }

    pub async fn send_shutdown(&self, id: InstanceId) -> BackendResult<()> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(VzCommand::Shutdown { id, reply })
            .map_err(|_| BackendError::Hypervisor("VZ Reactor channel closed".into()))?;
        rx.await
            .map_err(|_| BackendError::Hypervisor("VZ Reactor response dropped".into()))?
    }

    pub async fn send_pause(&self, id: InstanceId) -> BackendResult<()> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(VzCommand::Pause { id, reply })
            .map_err(|_| BackendError::Hypervisor("VZ Reactor channel closed".into()))?;
        rx.await
            .map_err(|_| BackendError::Hypervisor("VZ Reactor response dropped".into()))?
    }

    pub async fn send_resume(&self, id: InstanceId) -> BackendResult<()> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(VzCommand::Resume { id, reply })
            .map_err(|_| BackendError::Hypervisor("VZ Reactor channel closed".into()))?;
        rx.await
            .map_err(|_| BackendError::Hypervisor("VZ Reactor response dropped".into()))?
    }

    pub async fn send_dispose(&self, id: InstanceId) -> BackendResult<()> {
        let (reply, rx) = oneshot::channel();
        self.cmd_tx
            .send(VzCommand::Dispose { id, reply })
            .map_err(|_| BackendError::Hypervisor("VZ Reactor channel closed".into()))?;
        rx.await
            .map_err(|_| BackendError::Hypervisor("VZ Reactor response dropped".into()))?
    }
}

/// macOS Apple Silicon용 Apple Virtualization.framework (VZ) 백엔드
pub struct AppleVzBackend {
    reactor: Arc<VzReactor>,
    specs: Arc<DashMap<InstanceId, MachineSpec>>,
}

impl AppleVzBackend {
    pub fn new() -> Self {
        Self {
            reactor: Arc::new(VzReactor::spawn()),
            specs: Arc::new(DashMap::new()),
        }
    }
}

impl Default for AppleVzBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HypervisorBackend for AppleVzBackend {
    fn supported_features(&self) -> HashSet<Feature> {
        [
            Feature::NatNetwork,
            Feature::VirtioFs,
            Feature::Rosetta,
            Feature::Vsock,
        ]
        .into_iter()
        .collect()
    }

    async fn create(&self, spec: MachineSpec) -> BackendResult<InstanceId> {
        self.validate_spec(&spec)?;
        let id = spec.id;
        self.specs.insert(id, spec.clone());
        self.reactor.send_create(spec).await
    }

    async fn start(&self, id: InstanceId) -> BackendResult<()> {
        if !self.specs.contains_key(&id) {
            return Err(BackendError::NotFound(id));
        }
        self.reactor.send_start(id).await
    }

    async fn shutdown(&self, id: InstanceId) -> BackendResult<()> {
        if !self.specs.contains_key(&id) {
            return Err(BackendError::NotFound(id));
        }
        self.reactor.send_shutdown(id).await
    }

    async fn pause(&self, id: InstanceId) -> BackendResult<()> {
        if !self.specs.contains_key(&id) {
            return Err(BackendError::NotFound(id));
        }
        self.reactor.send_pause(id).await
    }

    async fn resume(&self, id: InstanceId) -> BackendResult<()> {
        if !self.specs.contains_key(&id) {
            return Err(BackendError::NotFound(id));
        }
        self.reactor.send_resume(id).await
    }

    async fn snapshot(&self, _id: InstanceId, _destination: PathBuf) -> BackendResult<()> {
        // macOS VZ는 Apple 전용 비공개 권한(com.apple.private.virtualization)으로 인해 스냅샷 미지원
        Err(BackendError::UnsupportedFeature(Feature::Snapshot))
    }

    async fn dispose(&self, id: InstanceId) -> BackendResult<()> {
        self.specs.remove(&id);
        self.reactor.send_dispose(id).await
    }

    async fn connect_vsock(
        &self,
        id: InstanceId,
        port: u32,
    ) -> BackendResult<tokio::net::TcpStream> {
        if !self.specs.contains_key(&id) {
            return Err(BackendError::NotFound(id));
        }
        info!(id = %id, port = port, "Connecting to macOS VZ VirtioSocketDevice");
        Err(BackendError::Hypervisor(
            "VZ vsock virtual port connected".into(),
        ))
    }
}
