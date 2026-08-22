# MOS 시스템 세부 구현 규격 (SPEC.md)

---

## 1. 프로젝트 크레이트 구조 (Rust Cargo Workspace)

```
mos/
├── Cargo.toml                  # Workspace Root
├── docs/
│   ├── ARCHITECTURE.md         # 전체 아키텍처 및 상태 머신
│   └── SPEC.md                 # 세부 구현 규격 및 API 명세
├── crates/
│   ├── mos-core/               # 공통 도메인 모델, 에러 타입, 상태 정의
│   ├── mos-orchestrator/       # Firecracker 생명주기, TAP IPAM, cgroups v2
│   ├── mos-edge/               # Pingora/Hyper 기반 Ingress 프록시 & Wake-on-HTTP
│   ├── mos-builder/            # Nixpacks 래퍼 및 ext4 Rootfs 생성기
│   ├── mos-init/               # Guest MicroVM PID 1 초경량 init 바이너리
│   └── mos-cli/                # 개발자용 CLI (mos deploy, mos logs 등)
├── scripts/
│   ├── fetch-kernel.sh         # Firecracker 호환 Linux vmlinux 다운로드
│   ├── build-rootfs.sh         # Base Alpine Rootfs 생성 스크립트
│   └── test-e2e.sh             # E2E 기동 및 HTTP 라우팅 테스트 스크립트
└── runtime/                    # 로컬 런타임 저장소 (VM 소켓, 스냅샷, 루트fs)
    ├── kernels/
    ├── base-rootfs/
    ├── instances/
    └── snapshots/
```

---

## 2. 모듈별 기술 스택 및 의존성

| Crate | 주요 의존성 (Crater) | 용도 |
| :--- | :--- | :--- |
| **mos-core** | `serde`, `serde_json`, `thiserror`, `tokio`, `uuid`, `chrono` | 공통 타입 및 인터페이스 |
| **mos-orchestrator** | `hyper-util`, `hyper`, `tokio`, `nix`, `tracing`, `sysinfo` | Firecracker Socket IPC, 프로세스 제어, TAP 생성 |
| **mos-edge** | `hyper`, `tokio`, `rustls`, `bytes`, `dashmap`, `futures-util` | Ingress Proxy, 버퍼링, 지연 라우팅 |
| **mos-builder** | `nixpacks`, `tokio-process`, `tempfile`, `flate2`, `tar` | 코드 빌드 및 ext4 이미지 패키징 |
| **mos-init** | `nix`, `libc` (no_std 또는 최소 std) | 게스트 VM 내부 PID 1 Init |
| **mos-cli** | `clap`, `reqwest`, `tokio`, `indicatif`, `colored` | 사용자 CLI |

---

## 3. 네트워크 및 IPAM 규격

* **호스트 서브넷**: `172.16.0.0/16` (최대 65,534개 MicroVM 격리)
* **호스트 브리지 인터페이스**: `mos-br0` (`172.16.0.1`)
* **인스턴스별 TAP 인터페이스**: `tap-mos-001`, `tap-mos-002`, ...
* **게스트 IP 할당**:
  * VM #1: `172.16.0.2`
  * VM #2: `172.16.0.3`
  * Gateway: `172.16.0.1`
  * DNS: `1.1.1.1` (Cloudflare) / `8.8.8.8` (Google)
* **NAT / 포워딩**: `iptables -t nat -A POSTROUTING -s 172.16.0.0/16 ! -d 172.16.0.0/16 -j MASQUERADE`

---

## 4. Firecracker JSON API 규격 예시

```json
// 1. Boot Source 설정
PUT /boot-source
{
  "kernel_image_path": "/var/lib/mos/kernels/vmlinux.bin",
  "boot_args": "console=ttyS0 reboot=k panic=1 pci=off init=/init ip=172.16.0.2::172.16.0.1:255.255.0.0::eth0:off"
}

// 2. Root Drive 설정
PUT /drives/rootfs
{
  "drive_id": "rootfs",
  "path_on_host": "/var/lib/mos/instances/inst-01/rootfs.ext4",
  "is_root_device": true,
  "is_read_only": false
}

// 3. Network Interface 설정
PUT /network-interfaces/eth0
{
  "iface_id": "eth0",
  "guest_mac": "AA:FC:00:00:00:02",
  "host_dev_name": "tap-mos-001"
}

// 4. Machine Config (vCPU / Memory)
PUT /machine-config
{
  "vcpu_count": 1,
  "mem_size_mib": 256,
  "smt": false
}

// 5. Instance Start
PUT /actions
{
  "action_type": "InstanceStart"
}
```

---

## 5. Scale-to-Zero 스냅샷 규격

1. **스냅샷 생성 (Pause & Snapshot)**:
   ```json
   PATCH /vm
   { "state": "Paused" }

   PUT /snapshot/create
   {
     "snapshot_type": "Full",
     "snapshot_path": "/var/lib/mos/snapshots/inst-01.snap",
     "mem_file_path": "/var/lib/mos/snapshots/inst-01.mem"
   }
   ```
2. **스냅샷 복구 (Resume from Snapshot)**:
   ```json
   PUT /snapshot/load
   {
     "snapshot_path": "/var/lib/mos/snapshots/inst-01.snap",
     "mem_file_path": "/var/lib/mos/snapshots/inst-01.mem",
     "enable_diff_snapshots": false,
     "resume_vm": true
   }
   ```

---

## 6. Phase 8 확장 규격

### 6.1. 분산 공유 볼륨 (Shared Volume)
* **볼륨 접근 모드**: `ReadWriteOnce` (단일 VM 배타 잠금), `ReadWriteMany` (다중 VM 공유 쓰기), `ReadOnly`
* **호스트-게스트 바인딩**: Firecracker 드라이브 디바이스 및 게스트 내부 지정 경로(`/mnt/...`) 자동 마운트
* **테넌트 쿼터 제어**: 테넌트별 최대 볼륨 수 및 총 스토리지 용량(Byte) 제한 강제

### 6.2. 실시간 계량 및 크레딧 빌링 (Metered Billing Engine)
* **계량 항목**:
  * `vCPU Core-Seconds` (기본 $0.000010/sec ≈ $0.036/hr)
  * `RAM GiB-Seconds` (기본 $0.000002/sec ≈ $0.0072/GiB-hr)
  * `GPU VRAM GiB-Seconds` (기본 $0.000050/sec ≈ $0.18/GiB-hr)
  * `Egress Network Bytes` (기본 $0.05/GiB)
* **계정 제어**: 실시간 틱 기반 차감, 크레딧 한도 소진 시 인스턴스 자동 일시정지(Auto-Suspend)

### 6.3. GitOps & 3단계 카나리 자동 승격 파이프라인
* **GitHub Webhook**: HMAC-SHA256 서명 검증 및 Push 이벤트 기반 빌드 트리거
* **점진적 트래픽 시프팅**: `10% -> 50% -> 100%` 단계별 최소 요청 수(`min_requests_per_step`) 도달 시 자동 승격
* **결함 자가 치유 (Self-Healing Rollback)**: 5xx 에러율 임계치(기본 5.0%) 초과 시 즉각 이전 안정 버전(Stable 100%)으로 롤백
