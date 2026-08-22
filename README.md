<div align="center">

# 🦀 MOS (MicroVM Operating Service)

**A lightweight, hyper-dense, scale-to-zero serverless platform built on Linux KVM & Firecracker MicroVMs.**  
*Vibe Coders 및 고성능 클라우드 워크로드를 위한 Rust 네이티브 초경량·초고밀도 Scale-to-Zero PaaS*

[![CI](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml/badge.svg)](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64%20(KVM)-lightgrey.svg)](https://kernel.org)
[![Latency](https://img.shields.io/badge/Wake--on--HTTP-%3C%207.00ms-success.svg)](docs/BENCHMARK_REPORT.md)

[English](#overview) • [한국어](#개요) • [Quickstart](#quickstart) • [Architecture](#architecture) • [Benchmarks](#measured-benchmarks) • [Documentation](docs/)

</div>

---

## Overview

**MOS (MicroVM Operating Service)** is a next-generation, Rust-native serverless runtime and edge hosting platform. It provides instant application deployments from source code without Dockerfiles, guarantees hardware-enforced isolation through Linux KVM and AWS Firecracker MicroVMs, and scales idle workloads completely down to **0 MB RAM & 0 MB VRAM**.

When incoming HTTP traffic arrives for a suspended instance, MOS wakes the MicroVM within **1.20 ms (UFFD lazy restore)** or **6.57 ms (full snapshot)**, achieving sub-7ms end-to-end response delivery with zero dropped packets.

---

## 개요

**MOS**는 개발자가 `Dockerfile`이나 복잡한 Kubernetes 매니페스트 없이 소스코드만으로 수초 만에 배포할 수 있는 **초경량·초고밀도 Serverless PaaS**입니다.

컨테이너 커널 공유 방식의 보안 한계를 넘어 **Linux KVM + Firecracker MicroVM** 기반의 하드웨어 레벨 가상화 경계를 제공하며, 유휴 인스턴스를 메모리 스냅샷 상태로 전환하여 **0MB RAM 및 0MB GPU VRAM**의 진정한 Scale-to-Zero를 실현합니다.

---

## 🌟 Key Features

| Feature | Description |
| :--- | :--- |
| ⚡ **Sub-7ms Wake-on-HTTP** | 유휴 시 프로세스 완전 종료(0MB), 첫 HTTP 패킷 유입 시 **1.20ms (UFFD 지연 페이징)** / **6.57ms (스냅샷 복구)** 내 즉각 기상 및 패킷 포워딩 |
| 🔒 **Hardware-Enforced Isolation** | Linux KVM 하드웨어 가상화와 Firecracker MicroVM을 통한 커널 레벨 격리 (컨테이너 탈옥/취약점 완벽 차단) |
| 🚀 **Zero-Config Build & Deploy** | Rust 기반 Nixpacks 엔진 통합 — Node.js, Python, Rust, Go 등을 자동 감지하여 별도 설정 없이 즉시 ext4 Rootfs 빌드 |
| 💾 **SQLite-First & Litestream** | SQLite 파일 자동 감지 및 S3 / Cloudflare R2로 실시간 트랜잭션 스트리밍 백업 자동화 |
| 🌐 **Sub-millisecond Edge Ingress** | Hyper/Tokio 기반 고성능 리버스 프록시, 자동 ACME/TLS, 3단계 가중치 카나리 배포 (10% ➔ 50% ➔ 100%), HMAC 무중단 롤백 |
| 🛰️ **P2P Mesh Cluster** | SWIM Gossip 프로토콜과 일관된 해시 링(Consistent Hash Ring) 기반의 탈중앙 분산 노드 디스커버리 및 글로벌 라우팅 |
| 🎯 **Dynamic GPU VRAM Pooling** | LLM/AI 인퍼런스 워크로드에 대한 동적 VRAM 할당 및 유휴 시 0MB 완전 반납(Scale-to-Zero GPU) |
| 🛡️ **eBPF/XDP Defense & Ed25519 RBAC** | 커널 레벨 L4 DDoS 방어 필터 및 비대칭 암호화 서명 기반 무상태(Stateless) RBAC 인가 검증 (<0.01ms) |
| 📊 **Real-time Metering & Credit Billing** | vCPU, RAM, VRAM, 네트워크 Egress 초단위 정밀 계량 및 잔액 소진 시 자동 서스펜드 |

---

## 📊 Measured Benchmarks

실제 물리 베어메탈 서버(Linux 6.17, AMD Ryzen 7 9700X 8C/16T, KVM, NVMe SSD)에서 측정한 공인 실측 결과입니다 ([상세 벤치마크 보고서](docs/BENCHMARK_REPORT.md)).

```
┌───────────────────────────────────────────────┬─────────────────┬────────────────────────┐
│ Benchmark Metric                              │ Measured Time   │ Comparison vs Baseline │
├───────────────────────────────────────────────┼─────────────────┼────────────────────────┤
│ MicroVM Cold Boot (KVM Kernel Init)           │ 10.06 ms        │ 50-100x faster vs OCI  │
│ Guest PID 1 `mos-init` Early Boot             │ 1.15 ms         │ /proc, /sys, eth0 UP   │
│ Scale-to-Zero Memory Snapshot (128MB)         │ 123.97 ms       │ Full Memory Dump       │
│ Fast Snapshot Resume (Full Memory Map)        │ 6.57 ms         │ 20x faster vs Lambda   │
│ UFFD On-Demand Lazy Resume (Zstandard)        │ 1.20 ms         │ Sub-millisecond Resume │
│ End-to-End Wake-on-HTTP Roundtrip             │ < 7.00 ms       │ Buffer -> Wake -> Resp │
│ Ed25519 RBAC Token Stateless Verification     │ < 0.01 ms       │ 100,000+ ops/sec       │
│ eBPF XDP Packet Filter Overhead               │ < 0.02 ms       │ Kernel L4 Drop/Pass    │
│ Consistent Hash Ring Node Lookup              │ < 0.05 ms       │ O(log N) Binary Search │
│ Scale-to-Zero Idle Footprint (RAM / VRAM)     │ 0.0 MB          │ Zero Resource Waste    │
└───────────────────────────────────────────────┴─────────────────┴────────────────────────┘
```

---

## 🏗️ Architecture

MOS는 단일 모놀리스가 아닌 모듈형 Rust 워크스페이스(7개 독립 Crate)로 설계되었습니다.

```
                     [ Public Traffic / Clients ]
                                  │
                                  ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🌐 mos-edge (High-Performance Ingress Proxy)                │
   │    • eBPF / XDP DDoS Filter  • W3C Distributed Tracing      │
   │    • Request Buffering       • Wake-on-HTTP IPC Trigger     │
   │    • Automated TLS (ACME)    • 3-Stage Canary Pipeline      │
   └──────────────┬───────────────────────────────┬──────────────┘
                  │ UDS / IPC Signal              │ Direct Proxy
                  ▼                               ▼
   ┌───────────────────────────────┐  ┌──────────────────────────┐
   │ 🛰️ mos-cluster                │  │ 🏗️ mos-builder           │
   │    • SWIM Gossip Membership   │  │    • Nixpacks Detection  │
   │    • Consistent Hash Ring     │  │    • ext4 Rootfs Builder │
   │    • Global Cross-Node Router │  │    • Litestream Injector │
   └──────────────┬────────────────┘  └───────────┬──────────────┘
                  │                               │
                  ▼                               ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🎛️ mos-orchestrator (Host Controller & Node Daemon)          │
   │    • Firecracker v1.x Socket API Controller                 │
   │    • Memory Snapshot & UFFD Lazy Resume Engine              │
   │    • Dynamic GPU VRAM Pool Manager                          │
   │    • Cgroup v2 Quotas & TAP/IPAM Network Management         │
   │    • Shared RWO/RWX Volume Manager & Metered Billing Engine │
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

### Workspace Crates

* **[`crates/mos-core`](crates/mos-core)**: Core domain models, instance lifecycle state machine, Ed25519 RBAC authorization, credit billing engine, and ISTQB CTFL verified models.
* **[`crates/mos-orchestrator`](crates/mos-orchestrator)**: Node-level daemon managing Firecracker processes, memory snapshots, UFFD lazy paging, Cgroup v2 quotas, GPU VRAM pooling, and shared storage volumes.
* **[`crates/mos-edge`](crates/mos-edge)**: Sub-millisecond reverse proxy with TCP/HTTP buffering, Wake-on-HTTP IPC coordination, ACME/TLS management, and automated weighted canary rollouts.
* **[`crates/mos-builder`](crates/mos-builder)**: Zero-config build engine integrating Nixpacks, ext4 packaging, SQLite/Litestream auto-detection, and heavy-workload native asset resolution.
* **[`crates/mos-init`](crates/mos-init)**: Microscopic static PID 1 guest supervisor (<820KB) responsible for early VFS mounting, network link setup, zombie process reaping, and vsock IPC telemetry.
* **[`crates/mos-cluster`](crates/mos-cluster)**: Decentralized multi-node clustering using SWIM Gossip protocol and consistent hash ring routing.
* **[`crates/mos-cli`](crates/mos-cli)**: Developer CLI tool and Operator web dashboard (`mos deploy`, `mos host init`, `mos dashboard`, `mos edge`, `mos bench`).

---

## 🚀 Quickstart

### Prerequisites

* **Linux OS** (Ubuntu 22.04+, Debian 12+, Arch, etc.)
* **KVM Enabled**: Hardware virtualization support (`/dev/kvm` read/write access)
* **Cgroups v2** enabled
* **Rust Toolchain**: 1.80+ (`rustup default stable`)

### 1. Installation & Building

```bash
# Clone the repository
git clone https://github.com/entropyparadox-lab/mos.git
cd mos

# Build release binaries
cargo build --release

# (Optional) Link CLI to PATH
sudo ln -sf $(pwd)/target/release/mos /usr/local/bin/mos
```

### 2. Host Provisioning & Preflight Check

Verify host virtualization capabilities and prepare `/var/lib/mos` storage structure:

```bash
# Run host preflight check
mos host preflight

# Initialize host directories & systemd unit template
sudo mos host init --dir /var/lib/mos
```

### 3. Deploying an Application

MOS automatically inspects your repository, detects runtime dependencies, and launches the MicroVM:

```bash
# Deploy a Next.js, FastAPI, or Rust Axum app
mos deploy ./examples/vibe-nextjs-app

# Inspect running instances
mos list
```

### 4. Running the Edge Ingress Proxy

Launch the high-performance reverse proxy with W3C distributed tracing and custom domain routing ([Ingress & Routing Guide](docs/INGRESS_ROUTING.md)):

```bash
# Launch with default domain or static routing table (config/routes.json)
mos edge --port 8180 --upstream 127.0.0.1:8080 --domain myapp.local

# Or load declarative multi-domain routing table:
# cp config/routes.example.json config/routes.json
mos edge --port 8180 --config config/routes.json
```

### 5. Running the Vibe Coder Web Dashboard

Access the real-time telemetry and management UI:

```bash
mos dashboard --port 8080
# Open http://localhost:8080 in your browser
```

### 6. Running Built-in Benchmarks

Benchmark MicroVM cold boot, snapshot creation, and fast resume latency directly on your machine:

```bash
mos bench
```

---

## ⚙️ Configuration (`mos.toml`)

MOS operates in **Zero-Config mode by default** — applications deploy without any configuration file. When you need custom resource limits, domain routing, GPU VRAM pooling, or outbound firewall policies, place a `mos.toml` file in your project root ([Full Configuration Guide](docs/MOS_CONFIG_SPEC.md)).

```toml
# mos.toml (Optional - all fields have sensible defaults)
[app]
name = "my-service"

[resources]
vcpu = 2
memory_mib = 512
# gpu_vram_mib = 8192                # Scale-to-Zero GPU VRAM allocation

[network]
port = 3000
domain = "my-service.mos.local"
egress = "allow-all"                 # "allow-all" (default) or "whitelist-only"

# Outbound firewall whitelist (when egress = "whitelist-only")
allowed_outbound = [
    "o12345.ingest.sentry.io",
    "www.google-analytics.com",
    "generativelanguage.googleapis.com"
]

[storage.litestream]
enabled = true                       # Live SQLite backup to S3/Cloudflare R2
db_path = "app.db"
replica_type = "s3"
bucket = "my-app-db-replicas"

[scaling]
idle_timeout_seconds = 300
strategy = "uffd"                    # 1.2ms UFFD lazy resume
```

See [**`docs/MOS_CONFIG_SPEC.md`**](docs/MOS_CONFIG_SPEC.md) for full schema details and recipes, and [**`mos.example.toml`**](mos.example.toml) for a ready-to-copy template.

---

## 📁 Repository Structure

```
mos/
├── Cargo.toml                  # Workspace manifest (7 crates)
├── config/
│   └── routes.example.json     # Edge router domain mapping example template
├── crates/
│   ├── mos-core/               # Domain models, RBAC, Billing, ISTQB tests
│   ├── mos-orchestrator/       # Firecracker runtime, UFFD, Cgroup, GPU pool
│   ├── mos-edge/               # Ingress proxy, Wake-on-HTTP, eBPF, TLS
│   ├── mos-builder/            # Nixpacks engine, Rootfs builder, Litestream
│   ├── mos-init/               # Static PID 1 guest init binary (<820KB)
│   ├── mos-cluster/            # SWIM Gossip mesh & Consistent Hash Ring
│   └── mos-cli/                # Operator CLI & Web Dashboard
├── docs/
│   ├── ARCHITECTURE.md         # Detailed system architecture specification
│   ├── SPEC.md                 # Component interfaces, APIs & state diagrams
│   ├── BENCHMARK_REPORT.md     # Production GA performance verification report
│   ├── MOS_CONFIG_SPEC.md      # mos.toml configuration specification & recipes
│   └── INGRESS_ROUTING.md      # Edge ingress proxy & routing table guide
├── examples/
│   ├── vibe-nextjs-app/        # Sample Next.js 14 SSR fullstack app
│   ├── vibe-fastapi-app/       # Sample FastAPI + SQLite backend
│   └── vibe-axum-app/          # Sample Rust Axum microservice
├── scripts/
│   ├── setup-firecracker.sh    # Download Firecracker & guest kernel assets
│   └── poc-boot-test.sh        # Quick standalone verification script
├── LICENSE-MIT                 # MIT License
├── LICENSE-APACHE              # Apache 2.0 License
├── CONTRIBUTING.md             # Contribution guidelines & test setup
└── CHANGELOG.md                # Version release history
```

---

## 🧪 Testing & Verification

MOS follows rigorous quality standards with comprehensive unit, integration, adversarial, and ISTQB CTFL boundary tests.

```bash
# Run all workspace tests (51 tests)
cargo test --workspace

# Run code style & formatting checks
cargo fmt --check

# Run linter checks
cargo clippy --workspace --all-targets
```

---

## 🗺️ Roadmap

- [x] **Architecture & Formal Specification**: Core domain models, state machine, and API contracts
- [x] **Phase 1: Firecracker MicroVM & Ingress PoC**: KVM boot lifecycle and HTTP reverse proxy
- [x] **Phase 2: Scale-to-Zero & Builder Engine**: Memory snapshot (Wake-on-HTTP) & Nixpacks packaging
- [x] **Phase 3: Storage & Platform DX**: SQLite/Litestream S3 replication & Web Dashboard
- [x] **Phase 4: Guest Shim & 3-App E2E**: Static PID 1 `mos-init`, Cgroups v2, and multi-framework E2E
- [x] **Phase 5: Ingress TLS & Weighted Canary**: Automated ACME, 3-stage canary rollout, and HMAC rollback
- [x] **Phase 6: P2P Cluster & UFFD Acceleration**: SWIM Gossip mesh, ZSTD UFFD lazy resume, eBPF/OTel
- [x] **Phase 7: Baremetal Provisioning & Scale-to-Zero GPU**: Host installer, Ed25519 RBAC, dynamic GPU VRAM pooling
- [x] **Phase 8: Distributed Volumes, Metered Billing & GitOps**: Shared volumes (RWO/RWX), per-second billing, and GitOps pipeline
- [ ] **Phase 9: Edge Anycast BGP & Multi-Region Global Sync**: Autonomous cross-region replica synchronization

---

## 🤝 Contributing

We welcome contributions from the community! Please read our [**Contributing Guide (CONTRIBUTING.md)**](CONTRIBUTING.md) and [**Code of Conduct (CODE_OF_CONDUCT.md)**](CODE_OF_CONDUCT.md) before submitting pull requests.

---

## 📄 License

MOS is dual-licensed under either:

* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
