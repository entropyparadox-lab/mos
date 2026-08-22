# MOS (MicroVM Operating Service) 시스템 아키텍처 명세서

> **Vibe Coders를 위한 Rust 기반 초경량·초고밀도 Scale-to-Zero PaaS**

---

## 1. 시스템 핵심 설계 철학

1. **Zero-Config Developer Experience (0초 설정)**: Dockerfile/k8s 매니페스트 불필요. 소스코드 또는 Zip 업로드만으로 즉시 빌드 및 배포.
2. **True Scale-to-Zero (30ms Cold Start)**: 유휴 시 메모리 스냅샷(Memory Snapshot) 상태로 절전, 첫 HTTP 패킷 유입 시 10~30ms 내 복구 포워딩.
3. **Rust Native & Hyper-Density**: 모든 컴포넌트(Orchestrator, Proxy, CLI)를 Rust로 작성하여 런타임 오버헤드 극소화. 단일 물리 베어메탈 서버에서 수천 개의 독립 테넌트 격리 구동.
4. **Hardware-Enforced Isolation**: 컨테이너 커널 공유 방식이 아닌 Linux KVM + Firecracker MicroVM 기반의 하드웨어 레벨 보안 경계 보장.

---

## 2. 전체 시스템 구조도

```
[ Public Traffic (*.mos.dev) ]
            │
            ▼
┌─────────────────────────────────────────────────────────────┐
│ 1. mos-edge (Ingress & Reverse Proxy)                       │
│    - Rust (Pingora / Hyper / Axum)                          │
│    - Wildcard TLS (rustls + ACME)                           │
│    - TCP/HTTP Request Buffering                             │
│    - Wake-on-HTTP IPC Signal -> mos-orchestrator            │
│    - Internal Sub-millisecond Routing Table                 │
└──────────────┬──────────────────────────────────────────────┘
               │ Unix Domain Socket / Shared Memory / IPC
               ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. mos-orchestrator (Host Controller & Node Daemon)         │
│    - MicroVM 생명주기 관리 (Spawn, Pause, Snapshot, Resume) │
│    - Firecracker v1.x Socket API Controller                 │
│    - Cgroup v2 리소스 쿼터 제어 (CPU, RAM)                  │
│    - TAP Interface & eBPF/IPAM 네트워크 관리                 │
│    - Control Plane REST/gRPC API (Axum)                     │
└──────────────┬──────────────────────────────────────────────┘
               │ Local Disk (ext4) & Socket
               ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. mos-builder (Zero-Config Build Engine)                   │
│    - Rust Nixpacks 엔진 통합                                │
│    - Language/Framework Auto-Detection                      │
│    - Minimal ext4 Rootfs 이미지 패키징                      │
│    - Litestream / SQLite 마운트 자동화                       │
└─────────────────────────────────────────────────────────────┘
               │
   ┌───────────┴─────────────────────────────┐
   ▼                                         ▼
┌─────────────────────────┐       ┌─────────────────────────┐
│ Firecracker MicroVM #1  │       │ Firecracker MicroVM #2  │
│ - Guest Kernel: vmlinux │       │ - Guest Kernel: vmlinux │
│ - Rootfs: Alpine/ext4   │       │ - Rootfs: Alpine/ext4   │
│ - User App (Next.js)    │       │ - User App (FastAPI)    │
│ - mos-init (guest shim) │       │ - mos-init (guest shim) │
│ - TAP IP: 172.16.0.2    │       │ - TAP IP: 172.16.0.3    │
└─────────────────────────┘       └─────────────────────────┘
```

---

## 3. 핵심 컴포넌트 상세 명세

### 3.1. `mos-edge` (Ingress Proxy)
* **역할**: 클라이언트 HTTP/WebSocket 요청을 수신하고, 대상 App의 MicroVM 상태에 따라 트래픽을 즉시 포워딩하거나 기상(Wake-up) 후 전달.
* **주요 기능**:
  * **Routing Table Cache**: Subdomain(`app-id.mos.dev`) -> MicroVM 상태(`Running`, `Suspended`, `Stopped`, `Building`) 및 내부 IP(`172.16.x.x:PORT`) 매핑.
  * **Request Buffering**: 대상 VM이 `Suspended` 상태인 경우, HTTP Body 및 헤더를 메모리 버퍼에 보관하고 즉시 `mos-orchestrator`에 `Resume(app-id)` IPC 요청 전송.
  * **Zero-Drop Resume Handshake**: VM 복구 완료 이벤트 수신 즉시 버퍼링된 요청을 전달 (총 지연시간 목표: 30ms 이하).

### 3.2. `mos-orchestrator` (Host Daemon)
* **역할**: 단일 노드 내 Firecracker MicroVM 프로세스 및 호스트 리소스 관리.
* **주요 기능**:
  * **Firecracker Process Management**: 각 VM 별 전용 Unix Domain Socket(`firecracker.sock`)을 통해 Firecracker와 통신 (`PutGuestBootSource`, `PutGuestDrive`, `PutGuestNetworkInterface`, `InstanceStart`, `CreateSnapshot`, `LoadSnapshot`).
  * **Scale-to-Zero State Machine**:
    * 활성 상태 -> 마지막 요청 후 N초(기본 300초) 무트래픽 감지 -> Memory Snapshot 생성(`vm.snap`, `vm.mem`) -> Firecracker 프로세스 종료 -> 디스크 상태 보존.
    * Ingress 기상 신호 수신 -> `LoadSnapshot` 파라미터로 Firecracker 실행 -> 메모리 매핑 즉시 복구(10~25ms) -> Running 전환.
  * **IPAM & TAP Management**: MicroVM 당 고유 Linux TAP 디바이스(`tap-mos-XX`) 생성 및 서브넷 IP 바인딩, iptables/eBPF 포워딩 규칙 적용.

### 3.3. `mos-builder` (Zero-Config Packaging)
* **역할**: 사용자의 Git 저장소 또는 Tarball을 수신하여 Firecracker에서 즉시 부팅 가능한 `rootfs.ext4` 생성.
* **빌드 파이프라인**:
  1. 소스코드 분석 (Nixpacks Plan 생성: 언어, 런타임 버전, 의존성 설치 커맨드, 빌드 커맨드, 시작 커맨드).
  2. 컨테이너/샌드박스 내부에서 빌드 산출물 생성.
  3. 사전에 준비된 기본 `mos-base-rootfs.ext4`(Alpine Linux + mos-init shim 포함)에 빌드 산출물 오버레이(Overlay/Copy).
  4. 메타데이터(`mos.json`: 포트, 실행 인자, 환경변수) 기록 후 스토리지 등록.

### 3.4. `mos-init` (Guest Shim)
* **역할**: MicroVM 내부 PID 1(init)으로 동작하는 초경량 Rust 바이너리.
* **크기**: 정적 컴파일 기준 1MB 미만.
* **동작**:
  * 루트 마운트 및 `/proc`, `/sys`, `/dev` 마운트 (1~2ms).
  * 네트워크 eth0 인터페이스 DHCP 또는 정적 IP 활성화 (1ms).
  * 사용자 앱 프로세스(예: `node server.js` 또는 `python main.py`) 실행 및 stdout/stderr 캡처.
  * 호스트와 vsock 또는 시리얼 콘솔을 통해 상태 및 헬스체크 신호 교환.

---

## 4. Scale-to-Zero 상태 머신

```
                  ┌──────────────────────┐
                  │       BUILDING       │
                  └──────────┬───────────┘
                             │ Build Success
                             ▼
                  ┌──────────────────────┐
                  │       STARTING       │ ◄────────────────┐
                  └──────────┬───────────┘                  │
                             │ Ready (Port Listening)       │
                             ▼                              │ Wake-on-HTTP
                  ┌──────────────────────┐                  │ (< 30ms)
       ┌─────────►│       RUNNING        │                  │
       │          └──────────┬───────────┘                  │
       │ Traffic In          │ Idle Timeout (300s)          │
       │                     ▼                              │
       │          ┌──────────────────────┐                  │
       │          │     SNAPSHOT-ING     │                  │
       │          └──────────┬───────────┘                  │
       │                     │ Snapshot Saved               │
       │                     ▼                              │
       │          ┌──────────────────────┐                  │
       └──────────┤      SUSPENDED       ├──────────────────┘
                  │ (Scale-to-Zero: 0MB) │
                  └──────────┬───────────┘
                             │ Manual Stop / Delete
                             ▼
                  ┌──────────────────────┐
                  │       STOPPED        │
                  └──────────────────────┘
```

---

## 5. 단계별 개발 계획 (Phased Milestones)

* **Phase 1: Firecracker MicroVM & Ingress PoC** (완료)
  * Firecracker 커널(`vmlinux`) 및 최소 Alpine `rootfs.ext4` 수급 및 검증
  * Rust 기반 Firecracker Spawn/Control 라이브러리 구현
  * 기본 Ingress Proxy 및 서브도메인 라우팅 PoC
* **Phase 2: Scale-to-Zero & Builder Engine** (완료)
  * Memory Snapshot / Resume 벤치마크 및 오토메이션
  * Ingress Request Buffering (Wake-on-HTTP)
  * Nixpacks 연동 소스코드 -> rootfs 변환 파이프라인
* **Phase 3: Storage & Platform DX** (완료)
  * SQLite + Litestream S3/R2 자동 스트리밍 백업
  * Wildcard TLS 및 도메인 관리
  * CLI (`mos deploy`, `mos logs`, `mos status`) 및 웹 대시보드
* **Phase 4: Guest Shim, Cgroups v2 & 3-App E2E** (완료)
  * 초경량 `mos-init` (PID 1) 및 AF_VSOCK IPC 통합
  * Cgroups v2 메모리/CPU 격리 및 Token Bucket 대역폭 제어
  * Next.js 14, FastAPI, Axum 3종 Vibe App 빌드 & 라우팅 실검증
* **Phase 5: Ingress TLS 전략, 가중치 카나리 & 웹 콘솔** (완료)
  * Auto-ACME, Self-Signed, Offload 전략 패턴 TLS 엔진
  * 가중치 기반 카나리 라우팅 (10%->50%->100%) 및 Webhook 연동
  * Zero-Dependency 웹 콘솔 대시보드 및 WebSocket 실시간 스트리밍
* **Phase 6: P2P 클러스터, UFFD 복구 가속 & eBPF/OTel** (완료)
  * SWIM 기반 Gossip 프로토콜 & Consistent Hashing 분산 Ingress
  * Userfaultfd (UFFD) 지연 페이징 및 ZSTD 스냅샷 압축 (<1.5ms)
  * eBPF XDP DDoS 커널 보안 필터 & OpenTelemetry W3C 분산 트레이싱
* **Phase 7: Baremetal 프로비저닝, RBAC 보안 & Scale-to-Zero GPU** (완료)
  * 원클릭 Baremetal 인스톨러 (`mos host init`) 및 Systemd 데몬 프로비저닝
  * 멀티테넌트 Namespace 쿼터 & Ed25519 비대칭 암호화 RBAC 토큰
  * Scale-to-Zero GPU / AI VRAM 동적 풀링 및 디태치 엔진
* **Phase 8: 분산 공유 볼륨, 실시간 계량 빌링 & GitOps 자동 카나리 승격** (완료)
  * ReadWriteMany / ReadWriteOnce 분산 공유 볼륨 마운트 및 쿼터 격리
  * vCPU/RAM/VRAM/Egress 초단위 실시간 계량 및 크레딧 빌링/오토 서스펜드
  * GitHub Push Webhook 기반 3단계 카나리 자동 점진 승격 & 5xx 에러율 기반 자동 롤백
