use crate::InstanceId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use thiserror::Error;

/// 하이퍼바이저 플랫폼별 지원 기능 플래그
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    /// 실시간 메모리 및 디바이스 상태 스냅샷 저장
    Snapshot,
    /// 캡처된 메모리 스냅샷으로부터의 고속 복구
    SnapshotRestore,
    /// Userfaultfd (UFFD) 기반 온디맨드 지연 페이징 복구
    UffdLazyRestore,
    /// Linux 호스트 TAP 네트워크 디바이스 (Firecracker)
    TapNetwork,
    /// macOS 내장 NAT 네트워크 디바이스 (Virtualization.framework)
    NatNetwork,
    /// Virtio-FS 디렉터리 마운트 및 파일 공유
    VirtioFs,
    /// Apple Silicon 환경의 x86_64 바이너리 Rosetta 번역
    Rosetta,
    /// AF_VSOCK 가상 소켓 통신
    Vsock,
    /// 호스트 데몬 재시작 시 실행 중인 VM 재입양(Adoption)
    Adoption,
    /// Scale-to-Zero GPU / VRAM 동적 풀링
    GpuVramPooling,
    /// eBPF XDP 커널 패킷 필터링
    EbpfXdpFilter,
}

impl std::fmt::Display for Feature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Feature::Snapshot => write!(f, "snapshot"),
            Feature::SnapshotRestore => write!(f, "snapshot_restore"),
            Feature::UffdLazyRestore => write!(f, "uffd_lazy_restore"),
            Feature::TapNetwork => write!(f, "tap_network"),
            Feature::NatNetwork => write!(f, "nat_network"),
            Feature::VirtioFs => write!(f, "virtio_fs"),
            Feature::Rosetta => write!(f, "rosetta"),
            Feature::Vsock => write!(f, "vsock"),
            Feature::Adoption => write!(f, "adoption"),
            Feature::GpuVramPooling => write!(f, "gpu_vram_pooling"),
            Feature::EbpfXdpFilter => write!(f, "ebpf_xdp_filter"),
        }
    }
}

/// 네트워크 인터페이스 설정
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkSpec {
    pub interface_name: String,
    pub mac_address: Option<String>,
    pub ip_address: Option<String>,
    pub host_dev_name: Option<String>,
    pub is_nat: bool,
}

impl NetworkSpec {
    pub fn tap(host_tap: impl Into<String>, guest_ip: impl Into<String>) -> Self {
        Self {
            interface_name: "eth0".into(),
            mac_address: None,
            ip_address: Some(guest_ip.into()),
            host_dev_name: Some(host_tap.into()),
            is_nat: false,
        }
    }

    pub fn nat() -> Self {
        Self {
            interface_name: "eth0".into(),
            mac_address: None,
            ip_address: None,
            host_dev_name: None,
            is_nat: true,
        }
    }
}

/// 추가 디스크 / 마운트 설정
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtraDiskSpec {
    pub drive_id: String,
    pub path_on_host: PathBuf,
    pub is_read_only: bool,
    pub is_root_device: bool,
}

/// 플랫폼 독립적 MicroVM 하드웨어 및 런타임 사양 (MachineSpec)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineSpec {
    pub id: InstanceId,
    pub name: String,
    pub vcpu_count: u8,
    pub mem_size_mib: u32,
    pub kernel_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub kernel_boot_args: String,
    pub networks: Vec<NetworkSpec>,
    pub extra_disks: Vec<ExtraDiskSpec>,
    pub vsock_port: Option<u32>,
    pub enable_rosetta: bool,
    pub restore_from_snapshot: Option<PathBuf>,
    pub created_at: DateTime<Utc>,
}

impl MachineSpec {
    pub fn new(name: impl Into<String>, kernel: PathBuf, rootfs: PathBuf) -> Self {
        Self {
            id: InstanceId::new(),
            name: name.into(),
            vcpu_count: 1,
            mem_size_mib: 128,
            kernel_path: kernel,
            rootfs_path: rootfs,
            kernel_boot_args: "console=ttyS0 reboot=k panic=1 pci=off nomodules rw".into(),
            networks: Vec::new(),
            extra_disks: Vec::new(),
            vsock_port: Some(10700),
            enable_rosetta: false,
            restore_from_snapshot: None,
            created_at: Utc::now(),
        }
    }

    pub fn with_vcpu(mut self, vcpu: u8) -> Self {
        self.vcpu_count = vcpu;
        self
    }

    pub fn with_memory(mut self, mib: u32) -> Self {
        self.mem_size_mib = mib;
        self
    }

    pub fn with_network(mut self, net: NetworkSpec) -> Self {
        self.networks.push(net);
        self
    }

    pub fn with_vsock(mut self, port: u32) -> Self {
        self.vsock_port = Some(port);
        self
    }

    pub fn with_snapshot_restore(mut self, snapshot_path: PathBuf) -> Self {
        self.restore_from_snapshot = Some(snapshot_path);
        self
    }
}

/// 하이퍼바이저 런타임 에러
#[derive(Error, Debug)]
pub enum BackendError {
    #[error("Unsupported feature on this host/backend: {0}")]
    UnsupportedFeature(Feature),

    #[error("Invalid MachineSpec: {0}")]
    InvalidSpec(String),

    #[error("Hypervisor operation error: {0}")]
    Hypervisor(String),

    #[error("MicroVM not found: {0}")]
    NotFound(InstanceId),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Timeout: {0}")]
    Timeout(String),
}

pub type BackendResult<T> = Result<T, BackendError>;

/// 공통 하이퍼바이저 백엔드 제어 인터페이스 (Linux KVM / macOS Apple VZ)
#[async_trait::async_trait]
pub trait HypervisorBackend: Send + Sync + 'static {
    /// 호스트 및 백엔드에서 지원하는 기능 목록 반환
    fn supported_features(&self) -> HashSet<Feature>;

    /// 기능 지원 여부 검사
    fn supports(&self, feature: Feature) -> bool {
        self.supported_features().contains(&feature)
    }

    /// MachineSpec 유효성 검사 (지원하지 않는 Feature 요청 시 에러)
    fn validate_spec(&self, spec: &MachineSpec) -> BackendResult<()> {
        if spec.restore_from_snapshot.is_some() && !self.supports(Feature::SnapshotRestore) {
            return Err(BackendError::UnsupportedFeature(Feature::SnapshotRestore));
        }
        if spec.enable_rosetta && !self.supports(Feature::Rosetta) {
            return Err(BackendError::UnsupportedFeature(Feature::Rosetta));
        }
        for net in &spec.networks {
            if net.is_nat && !self.supports(Feature::NatNetwork) {
                return Err(BackendError::UnsupportedFeature(Feature::NatNetwork));
            }
            if !net.is_nat && !self.supports(Feature::TapNetwork) {
                return Err(BackendError::UnsupportedFeature(Feature::TapNetwork));
            }
        }
        Ok(())
    }

    /// MicroVM 인스턴스 생성 및 초기화
    async fn create(&self, spec: MachineSpec) -> BackendResult<InstanceId>;

    /// MicroVM 부팅 및 실행
    async fn start(&self, id: InstanceId) -> BackendResult<()>;

    /// MicroVM 정상 종료 (Graceful Shutdown)
    async fn shutdown(&self, id: InstanceId) -> BackendResult<()>;

    /// MicroVM 일시 정지 (Pause vCPUs)
    async fn pause(&self, id: InstanceId) -> BackendResult<()>;

    /// MicroVM 재개 (Resume vCPUs)
    async fn resume(&self, id: InstanceId) -> BackendResult<()>;

    /// 메모리 및 디바이스 상태 스냅샷 캡처
    async fn snapshot(&self, id: InstanceId, destination: PathBuf) -> BackendResult<()>;

    /// MicroVM 리소스 해제 및 프로세스 정리
    async fn dispose(&self, id: InstanceId) -> BackendResult<()>;

    /// VSOCK 포트 연결 스트림 생성
    async fn connect_vsock(
        &self,
        id: InstanceId,
        port: u32,
    ) -> BackendResult<tokio::net::TcpStream>;
}

/// 단위 테스트용 인메모리 Mock 하이퍼바이저 백엔드
#[derive(Debug, Default)]
pub struct MockHypervisorBackend {
    supported: HashSet<Feature>,
}

impl MockHypervisorBackend {
    pub fn new(supported: impl IntoIterator<Item = Feature>) -> Self {
        Self {
            supported: supported.into_iter().collect(),
        }
    }
}

#[async_trait::async_trait]
impl HypervisorBackend for MockHypervisorBackend {
    fn supported_features(&self) -> HashSet<Feature> {
        self.supported.clone()
    }

    async fn create(&self, spec: MachineSpec) -> BackendResult<InstanceId> {
        self.validate_spec(&spec)?;
        Ok(spec.id)
    }

    async fn start(&self, _id: InstanceId) -> BackendResult<()> {
        Ok(())
    }

    async fn shutdown(&self, _id: InstanceId) -> BackendResult<()> {
        Ok(())
    }

    async fn pause(&self, _id: InstanceId) -> BackendResult<()> {
        if !self.supports(Feature::Snapshot) {
            return Err(BackendError::UnsupportedFeature(Feature::Snapshot));
        }
        Ok(())
    }

    async fn resume(&self, _id: InstanceId) -> BackendResult<()> {
        Ok(())
    }

    async fn snapshot(&self, _id: InstanceId, _dest: PathBuf) -> BackendResult<()> {
        if !self.supports(Feature::Snapshot) {
            return Err(BackendError::UnsupportedFeature(Feature::Snapshot));
        }
        Ok(())
    }

    async fn dispose(&self, _id: InstanceId) -> BackendResult<()> {
        Ok(())
    }

    async fn connect_vsock(
        &self,
        _id: InstanceId,
        _port: u32,
    ) -> BackendResult<tokio::net::TcpStream> {
        if !self.supports(Feature::Vsock) {
            return Err(BackendError::UnsupportedFeature(Feature::Vsock));
        }
        Err(BackendError::Hypervisor(
            "Mock vsock connect requires live listener".into(),
        ))
    }
}
