<div align="center">

# 🦀 MOS (MicroVM Operating Service)

**Linux KVM 및 Firecracker MicroVM 기반의 초경량·초고밀도 Scale-to-Zero Serverless PaaS**

[![CI](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml/badge.svg)](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64%20(KVM)-lightgrey.svg)](https://kernel.org)
[![Latency](https://img.shields.io/badge/Wake--on--HTTP-%3C%207.00ms-success.svg)](docs/BENCHMARK_REPORT.md)

**[ English ](README.md)** • **[ 한국어 ](README.ko.md)** • **[ 日本語 ](README.ja.md)** • **[ 简体中文 ](README.zh.md)**

[개요](#개요) • [핵심 특징](#-핵심-특징) • [실측 벤치마크](#-실측-성능-벤치마크) • [아키텍처](#%EF%B8%8F-시스템-아키텍처) • [빠른 시작](#-빠른-시작-quickstart) • [설정 가이드](#%EF%B8%8F-설정-가이드-mostoml) • [문서 링크](docs/)

</div>

---

## 개요

**MOS (MicroVM Operating Service)**는 Vibe Coder 및 고성능 클라우드 워크로드를 위한 Rust 네이티브 서버less 런타임 및 에지 호스팅 플랫폼입니다.

Dockerfile이나 복잡한 Kubernetes 매니페스트 없이 소스코드만으로 즉시 빌드 및 배포할 수 있으며, **Linux KVM + AWS Firecracker MicroVM** 하드웨어 가상화 경계를 통해 컨테이너 커널 공유 방식의 보안 취약점을 원천 차단합니다. 또한 유휴 시 메모리 스냅샷 상태로 전환하여 **0 MB RAM 및 0 MB GPU VRAM**의 진정한 Scale-to-Zero를 제공합니다.

유휴 상태의 인스턴스로 HTTP 트래픽이 유입되면, MOS는 **1.20 ms (Userfaultfd 지연 페이징)** 또는 **6.57 ms (풀 메모리 스냅샷 복구)** 내에 MicroVM을 기상시켜 패킷 유실 없이 7ms 이내에 즉각 응답을 반환합니다.

---

## 🌟 핵심 특징

| 특징 | 설명 |
| :--- | :--- |
| ⚡ **Sub-7ms Wake-on-HTTP** | 유휴 시 0 MB 완전 절전; HTTP 요청 수신 시 **1.20 ms (UFFD 지연 페이징)** / **6.57 ms (스냅샷 복구)** 내 즉시 기상 및 패킷 포워딩 |
| 🔒 **하드웨어 레벨 보안 격리** | Linux KVM 가상화와 Firecracker 경계를 통해 컨테이너 탈옥(Escape) 및 커널 취약점 위험 원천 배제 |
| 🚀 **0초 설정 빌드 & 배포 (Zero-Config)** | Rust Nixpacks 엔진 통합 — Node.js, Python, Rust, Go 등을 자동 감지하여 `Dockerfile` 없이 최소 ext4 Rootfs 빌드 |
| 💾 **SQLite-First & Litestream** | SQLite 데이터베이스 자동 감지 및 S3 / Cloudflare R2로 실시간 트랜잭션 스트리밍 백업 자동화 |
| 🌐 **초고속 에지 인그레스 (Edge Ingress)** | Hyper/Tokio 기반 고성능 리버스 프록시, ACME/TLS 자동화, 3단계 가중치 카나리 배포 (`10% -> 50% -> 100%`), HMAC 즉시 롤백 |
| 🛰️ **탈중앙 P2P 메쉬 클러스터** | SWIM Gossip 프로토콜 및 Consistent Hash Ring 기반의 분산 노드 디스커버리 및 크로스 노드 라우팅 |
| 🎯 **동적 Scale-to-Zero GPU 풀링** | AI/LLM 인퍼런스 워크로드에 대한 GPU VRAM 동적 할당 및 유휴 시 0 MB 완전 반납 |
| 🛡️ **eBPF/XDP 커널 방어 & Ed25519 RBAC** | 커널 레벨 L4 DDoS 방어 필터 및 비대칭 암호화 서명 기반 무상태(Stateless) RBAC 인가 검증 (<0.01 ms) |
| 📊 **실시간 계량 및 크레딧 과금 엔진** | vCPU, RAM, VRAM, Egress 초단위 정밀 계량 및 잔액 소진 시 자동 서스펜드 |

---

## 📊 실측 성능 벤치마크

물리 베어메탈 서버(Linux 6.17, AMD Ryzen 7 9700X 8C/16T, KVM, NVMe SSD)에서 실측한 공인 벤치마크 지표입니다 ([상세 벤치마크 보고서](docs/BENCHMARK_REPORT.md)).

```
┌───────────────────────────────────────────────┬─────────────────┬────────────────────────┐
│ 측정 항목                                     │ 실측값          │ 기준선 대비 성능 비교  │
├───────────────────────────────────────────────┼─────────────────┼────────────────────────┤
│ MicroVM Cold Boot (KVM Kernel Init)           │ 10.06 ms        │ 기존 컨테이너 대비 50-100배 고속 │
│ Guest PID 1 `mos-init` 초기 기동              │ 1.15 ms         │ /proc, /sys, eth0 마운트 완료 │
│ Scale-to-Zero 메모리 스냅샷 (128MB)           │ 123.97 ms       │ Full Memory Dump       │
│ Fast Snapshot Resume (Full Memory Map)        │ 6.57 ms         │ AWS Lambda 대비 20배 고속 │
│ UFFD On-Demand Lazy Resume (Zstandard)        │ 1.20 ms         │ 1ms 단위 초고속 복구   │
│ End-to-End Wake-on-HTTP 지연시간              │ < 7.00 ms       │ 버퍼링 -> 기상 -> 응답 │
│ Ed25519 RBAC 무상태 토큰 검증                 │ < 0.01 ms       │ 초당 10만 회 이상 처리 │
│ eBPF XDP 패킷 필터 오버헤드                   │ < 0.02 ms       │ 커널 L4 즉각 드롭/통과 │
│ Consistent Hash Ring 노드 분기 조회           │ < 0.05 ms       │ O(log N) 이진 탐색     │
│ Scale-to-Zero 유휴 점유율 (RAM / VRAM)        │ 0.0 MB          │ 리소스 낭비 제로       │
└───────────────────────────────────────────────┴─────────────────┴────────────────────────┘
```

---

## 🏗️ 시스템 아키텍처

MOS는 7개의 특화된 Rust Crate로 구성된 모듈형 워크스페이스입니다.

```
                     [ Public Traffic / Clients ]
                                  │
                                  ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🌐 mos-edge (초고성능 인그레스 프록시)                      │
   │    • eBPF / XDP DDoS 필터    • W3C 분산 트레이싱 전파       │
   │    • TCP/HTTP 요청 버퍼링    • Wake-on-HTTP IPC 트리거      │
   │    • 자동 TLS (ACME)         • 3단계 점진적 카나리 파이프라인│
   └──────────────┬───────────────────────────────┬──────────────┘
                  │ UDS / IPC Signal              │ Direct Proxy
                  ▼                               ▼
   ┌───────────────────────────────┐  ┌──────────────────────────┐
   │ 🛰️ mos-cluster                │  │ 🏗️ mos-builder           │
   │    • SWIM Gossip 노드 디스커버리│ │    • Nixpacks 자동 분석  │
   │    • Consistent Hash Ring     │  │    • ext4 Rootfs 패키징  │
   │    • 글로벌 크로스 노드 라우터│  │    • Litestream 복제 주입│
   └──────────────┬────────────────┘  └───────────┬──────────────┘
                  │                               │
                  ▼                               ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🎛️ mos-orchestrator (호스트 컨트롤러 & 노드 데몬)           │
   │    • Firecracker v1.x Socket API 컨트롤러                   │
   │    • 메모리 스냅샷 & UFFD 지연 페이징 엔진                  │
   │    • 동적 GPU VRAM 풀 관리자                                │
   │    • Cgroup v2 쿼터 및 TAP/IPAM 네트워크 관리               │
   │    • 공유 RWO/RWX 볼륨 관리자 & 계량 빌링 엔진              │
   └──────────────┬───────────────────────────────┬──────────────┘
                  │                               │
                  ▼                               ▼
       ┌─────────────────────┐         ┌─────────────────────┐
       │ Firecracker MicroVM │         │ Firecracker MicroVM │
       │  - Linux Kernel     │         │  - Linux Kernel     │
       │  - ext4 Rootfs      │         │  - ext4 Rootfs      │
       │  - 🚀 mos-init (PID 1)│        │  - 🚀 mos-init (PID 1)│
       │  - User App (Next.js)│       │  - User App (FastAPI)│
       └─────────────────────┘         └─────────────────────┘
```

### 워크스페이스 크레이트 구성

* **[`crates/mos-core`](crates/mos-core)**: 공통 도메인 모델, 상태 전이 매트릭스, Ed25519 RBAC 인가, 크레딧 과금 엔진, ISTQB CTFL 테스트 스위트
* **[`crates/mos-orchestrator`](crates/mos-orchestrator)**: Firecracker 제어, 메모리 스냅샷, UFFD 지연 페이징, Cgroups v2 격리, GPU VRAM 풀링, 공유 볼륨 관리
* **[`crates/mos-edge`](crates/mos-edge)**: Hyper 기반 고성능 리버스 프록시, TCP/HTTP 요청 버퍼링, Wake-on-HTTP IPC 연동, ACME/TLS, 3단계 카나리 롤아웃
* **[`crates/mos-builder`](crates/mos-builder)**: Nixpacks 통합 Zero-Config 패키징, ext4 빌더, SQLite/Litestream 복제 주입, 네이티브 조판 에셋 감지
* **[`crates/mos-init`](crates/mos-init)**: 820KB 미만의 초경량 정적 PID 1 게스트 슈퍼바이저 (VFS 마운트, 네트워크 구성, 좀비 프로세스 회수, vsock IPC)
* **[`crates/mos-cluster`](crates/mos-cluster)**: SWIM Gossip 프로토콜 기반 P2P 클러스터 디스커버리 및 Consistent Hash Ring 분산 라우팅
* **[`crates/mos-cli`](crates/mos-cli)**: 오퍼레이터 CLI 도구 및 실시간 웹 콘솔 대시보드 (`mos deploy`, `mos host init`, `mos dashboard`, `mos edge`, `mos bench`)

---

## 🚀 빠른 시작 (Quickstart)

### 사전 요구사항

* **Linux OS** (Ubuntu 22.04+, Debian 12+, Arch Linux 등)
* **KVM 활성화**: 하드웨어 가상화 권한 (`/dev/kvm` 읽기/쓰기 권한)
* **Cgroups v2** 활성화
* **Rust Toolchain**: 1.80+ (`rustup default stable`)

### 1. 설치 및 빌드

```bash
# 저장소 클론
git clone https://github.com/entropyparadox-lab/mos.git
cd mos

# 릴리즈 바이너리 빌드
cargo build --release

# (선택 사항) CLI 바이너리 PATH 링크
sudo ln -sf $(pwd)/target/release/mos /usr/local/bin/mos
```

### 2. 호스트 사전 진단 및 초기화

```bash
# 호스트 가상화 및 Cgroup 환경 진단
mos host preflight

# 호스트 디렉터리 구조(/var/lib/mos) 및 Systemd 유닛 생성
sudo mos host init --dir /var/lib/mos
```

### 3. 애플리케이션 배포

```bash
# Next.js, FastAPI, Rust Axum 프로젝트 배포
mos deploy ./examples/vibe-nextjs-app

# 실행 중인 인스턴스 목록 확인
mos list
```

### 4. Edge Ingress 프록시 실행

```bash
# 기본 도메인 또는 정적 라우팅 테이블(config/routes.json)로 실행
mos edge --port 8180 --upstream 127.0.0.1:8080 --domain myapp.local

# 또는 다중 도메인 라우팅 테이블 로드 (자세한 가이드: docs/INGRESS_ROUTING.md):
# cp config/routes.example.json config/routes.json
mos edge --port 8180 --config config/routes.json
```

### 5. 웹 대시보드 실행

```bash
mos dashboard --port 8080
# 브라우저에서 http://localhost:8080 접속
```

### 6. 내장 벤치마크 실행

```bash
mos bench
```

---

## ⚙️ 설정 가이드 (`mos.toml`)

MOS는 **기본적으로 Zero-Config 모드로 동작**합니다. 커스텀 리소스, 전용 도메인, GPU VRAM 할당, 이그레스 방화벽 설정이 필요할 때만 프로젝트 루트에 `mos.toml`을 작성합니다 ([전체 설정 명세서](docs/MOS_CONFIG_SPEC.md)).

```toml
# mos.toml (선택 사항 - 모든 필드는 기본값을 가집니다)
[app]
name = "my-service"

[resources]
vcpu = 2
memory_mib = 512
# gpu_vram_mib = 8192                # Scale-to-Zero GPU VRAM 할당

[network]
port = 3000
domain = "my-service.mos.local"
egress = "allow-all"                 # "allow-all" (기본값) 또는 "whitelist-only"

# 외부 방화벽 화이트리스트 (egress = "whitelist-only" 시)
allowed_outbound = [
    "o12345.ingest.sentry.io",
    "www.google-analytics.com",
    "generativelanguage.googleapis.com"
]

[storage.litestream]
enabled = true                       # SQLite S3/Cloudflare R2 실시간 복제
db_path = "app.db"
replica_type = "s3"
bucket = "my-app-db-replicas"

[scaling]
idle_timeout_seconds = 300
strategy = "uffd"                    # 1.2ms UFFD 지연 페이징 복구
```

자세한 스키마와 워크로드별 레시피는 [**`docs/MOS_CONFIG_SPEC.md`**](docs/MOS_CONFIG_SPEC.md) 및 [**`mos.example.toml`**](mos.example.toml)을 참고하세요.

---

## 📁 디렉터리 구조

```
mos/
├── Cargo.toml                  # Cargo Workspace 매니페스트 (7개 크레이트)
├── config/
│   └── routes.example.json     # Edge 라우터 도메인 매핑 템플릿
├── crates/
│   ├── mos-core/               # 공통 도메인 모델, RBAC, 빌링, ISTQB 테스트
│   ├── mos-orchestrator/       # Firecracker 런타임, UFFD, Cgroup, GPU 풀
│   ├── mos-edge/               # 인그레스 프록시, Wake-on-HTTP, eBPF, TLS
│   ├── mos-builder/            # Nixpacks 엔진, Rootfs 빌더, Litestream
│   ├── mos-init/               # 초경량 정적 PID 1 게스트 바이너리 (<820KB)
│   ├── mos-cluster/            # SWIM Gossip 메쉬 & Consistent Hash Ring
│   └── mos-cli/                # 오퍼레이터 CLI & 웹 대시보드
├── docs/
│   ├── ARCHITECTURE.md         # 시스템 아키텍처 명세서
│   ├── SPEC.md                 # 컴포넌트 인터페이스, API 및 상태 다이어그램
│   ├── BENCHMARK_REPORT.md     # Production GA 성능 실측 검증 보고서
│   ├── MOS_CONFIG_SPEC.md      # mos.toml 설정 명세서 및 레시피
│   └── INGRESS_ROUTING.md      # Edge 인그레스 프록시 및 라우팅 가이드
├── examples/
│   ├── vibe-nextjs-app/        # Next.js 14 SSR 풀스택 예제
│   ├── vibe-fastapi-app/       # FastAPI + SQLite 백엔드 예제
│   └── vibe-axum-app/          # Rust Axum 마이크로서비스 예제
├── scripts/
│   ├── setup-firecracker.sh    # Firecracker 바이너리 및 커널 다운로드
│   └── poc-boot-test.sh        # 독립 부팅 검증 스크립트
├── mos.example.toml            # mos.toml 설정 템플릿
├── LICENSE-MIT                 # MIT 라이센스
├── LICENSE-APACHE              # Apache 2.0 라이센스
├── CONTRIBUTING.md             # 컨트리뷰션 가이드 및 테스트 방법
└── CHANGELOG.md                # 버전 릴리즈 내역
```

---

## 🧪 테스트 및 품질 검증

```bash
# 전체 51개 테스트 스위트 실행
cargo test --workspace

# 코드 포맷팅 검증
cargo fmt --check

# Linter 검증
cargo clippy --workspace --all-targets
```

---

## 🤝 기여 안내

MOS 프로젝트에 대한 기여를 환영합니다! 기여 전 [**컨트리뷰션 가이드 (CONTRIBUTING.md)**](CONTRIBUTING.md) 및 [**행동강령 (CODE_OF_CONDUCT.md)**](CODE_OF_CONDUCT.md)을 확인해 주세요.

---

## 📄 라이센스

MOS는 사용자의 선택에 따라 다음 두 가지 라이센스로 듀얼 라이센싱됩니다:

* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
