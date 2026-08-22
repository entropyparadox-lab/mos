# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0] - 2026-08-22

### Initial Open Source Release (Production GA)

This marks the official open source release of **MOS (MicroVM Operating Service)** — a hyper-dense, Rust-native, scale-to-zero serverless platform built on Linux KVM and Firecracker MicroVMs.

### Added

#### Cross-Platform Hypervisor & Apple Silicon Support (`Phase 11`)
* **`mos-core` Hypervisor Backend Trait**: Hardware-independent `MachineSpec`, `MachineState`, and `HypervisorBackend` trait with compile-time target dispatch and granular `Feature` gating (Snapshot, UFFD, TAP, NAT, VirtioFS, Rosetta, Vsock, Adoption).
* **`mos-builder` Rootless OCI & EFI zboot Parser**: Pure Rust in-memory OCI layer unpacker with `.wh.` whiteout marker application, arm64 `vmlinuz` EFI zboot header parser (`MZ` + `zimg` -> raw `ARMd` Image), newc-cpio `initramfs` generator, and APFS `clonefile` CoW disk cloner.
* **`mos-orchestrator` Apple Virtualization.framework (VZ) Backend**: Single serial `DispatchQueue` Reactor pattern isolating `!Send + !Sync` VZVirtualMachine objects from Tokio worker threads with native NAT and Vsock device bindings.
* **`mos-edge` Multi-Platform WakeMode**: Subdomain routing engine with `WakeMode::SnapshotResume` (Linux UFFD 1.2ms) and `WakeMode::ColdBoot` (macOS VZ 15~25ms) buffering handshakes.
* **Phase 11 Cross-Platform E2E Test Suite**: Comprehensive cross-validation pipeline covering EFI zboot decompression, rootless OCI unpacking, CoW disk replication, VZ reactor gating, and cross-platform edge routing.

#### Core Architecture & MicroVM Orchestration (`mos-core`, `mos-orchestrator`)
* **KVM MicroVM Lifecycle Engine**: Hardware-enforced guest execution using Firecracker v1.x socket APIs (`/boot-source`, `/drives`, `/network-interfaces`, `/vsock`, `/actions`, `/snapshot/create`, `/snapshot/load`).
* **Scale-to-Zero Snapshot Engine**: Full memory snapshot creation with sub-30ms cold-resume capability.
* **Userfaultfd (UFFD) Lazy Paging Engine**: Sub-millisecond (1.20 ms) on-demand memory paging using Zstandard snapshot compression.
* **Dynamic Scale-to-Zero GPU Pool**: Dynamic GPU VRAM allocation for AI/LLM inference workloads with 0 MB idle memory release.
* **Cgroup v2 Resource Isolation**: Hard limits on CPU quotas (`cpu.max`) and memory limits (`memory.max`).
* **Shared Storage Volume Manager**: Support for Read-Write-Once (RWO) and Read-Write-Many (RWX) multi-tenant volume mounts with atomic locking.
* **Metered Credit Billing Engine**: Per-second accounting of vCPU, RAM, VRAM, and Egress with automated suspension on overdraft.
* **Ed25519 RBAC Authentication**: Cryptographically signed stateless access tokens with millisecond-grade verification.
* **ISTQB CTFL Certified Test Matrix**: Boundary Value Analysis (BVA), Decision Table testing, and state transition validation.

#### High-Performance Ingress & Traffic Management (`mos-edge`)
* **Sub-millisecond Edge Proxy**: Hyper/Tokio-based asynchronous reverse proxy with sub-millisecond route resolution.
* **Wake-on-HTTP Handshake**: Automatic request buffering during MicroVM idle state with instant wake signaling via Unix domain sockets.
* **Automated TLS (ACME HTTP-01)**: Dynamic wildcard certificate resolution and SNI certificate dispatch.
* **3-Stage Weighted Canary Rollouts**: Stepwise traffic promotion (`10% -> 50% -> 100%`) with automated error threshold detection and instant HMAC rollback.
* **eBPF / XDP DDoS Filter**: Kernel-level L4 packet filtering and rate-limiting.
* **W3C Distributed Tracing**: Traceparent header injection and granular per-phase latency breakdown (`ingress`, `routing`, `wake`, `guest_exec`).

#### Build Engine & Zero-Config Packaging (`mos-builder`)
* **Nixpacks Plan Integration**: Automatic detection and packaging of Node.js, Python, Rust, and Go applications.
* **Ext4 Rootfs Builder**: Preparation of minimal guest disk images with overlay capabilities.
* **SQLite-First & Litestream Support**: Automated SQLite database detection and live continuous replication streaming to S3/Cloudflare R2.
* **Heavy Native Workload Detector**: Specialized asset resolution for typesetting engines (Typst, rhwp) and CJK font bundles.

#### Guest PID 1 Supervisor (`mos-init`)
* **Static Binary (<820 KB)**: Ultra-compact init process written in pure Rust.
* **Early Virtual Filesystem Mounting**: Sub-millisecond mounting of `/proc`, `/sys`, `/dev`, `/run`, and `/tmp`.
* **Network Auto-Configuration**: Automated loopback and `eth0` IP address assignment.
* **Process Supervisor**: Application lifecycle monitoring, zombie reaping, and stdout/stderr capture.
* **Vsock Telemetry IPC**: Guest-to-host healthcheck and event streaming.

#### Decentralized Clustering (`mos-cluster`)
* **SWIM Gossip Protocol**: Decentralized node discovery, heartbeats, and failure detection.
* **Consistent Hash Ring**: Virtual node distribution for deterministic request partitioning and failover routing.
* **Global Cross-Node Routing**: Transparent proxy forwarding between cluster nodes.

#### Developer Tooling & CLI (`mos-cli`)
* **Operator CLI**: Comprehensive management commands (`deploy`, `host preflight`, `host init`, `list`, `status`, `dashboard`, `edge`, `bench`).
* **Vibe Coder Web Dashboard**: Real-time instance overview, CPU/memory telemetry, and live log streaming.
* **Host Safe Performance Suite**: End-to-end multi-cycle soak endurance harness and benchmark evaluation tools.
