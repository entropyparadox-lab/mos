# 🏗️ MOS (MicroVM Operating Service) Architecture Specification

> **A Lightweight, Hyper-Dense, Scale-to-Zero Serverless PaaS Built on Linux KVM & Firecracker MicroVMs**

---

## 1. System Design Philosophy

1. **Zero-Config Developer Experience**: No `Dockerfile` or Kubernetes manifests required. Applications are built and deployed directly from source code repositories in seconds.
2. **True Scale-to-Zero (<7ms Wake-on-HTTP)**: Idle workloads are automatically frozen to memory snapshots (0 MB RAM & 0 MB GPU VRAM). When an incoming HTTP request arrives, the MicroVM is resumed within 1.20 ms (via Userfaultfd lazy paging) or 6.57 ms (full snapshot restore) with zero dropped packets.
3. **Rust Native & Hyper-Density**: The entire control plane, ingress proxy, builder, and guest init are authored in Rust to minimize CPU and memory overhead, allowing thousands of isolated tenants to run concurrently on a single baremetal server.
4. **Hardware-Enforced Isolation**: Unlike container runtimes that share a common host kernel, MOS establishes strict hardware-level virtualization boundaries using Linux KVM and AWS Firecracker MicroVMs.

---

## 2. Global System Topology

```
                     [ Public Traffic (*.example.com) ]
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 1. mos-edge (Ingress & Reverse Proxy)                                       │
│    - High-performance asynchronous proxy (Hyper / Tokio)                    │
│    - Automated Wildcard TLS (rustls + ACME HTTP-01)                         │
│    - TCP/HTTP Request Buffering during Scale-to-Zero                        │
│    - Wake-on-HTTP IPC Signal -> mos-orchestrator                            │
│    - eBPF / XDP Kernel-level DDoS mitigation                                │
│    - 3-Stage Automated Weighted Canary Rollouts (10% -> 50% -> 100%)       │
└──────────────────────┬──────────────────────────────────────────────────────┘
                       │ Unix Domain Socket / IPC Signal
                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 2. mos-orchestrator (Host Controller & Node Daemon)                         │
│    - MicroVM Lifecycle Management (Spawn, Pause, Snapshot, Resume)         │
│    - Firecracker v1.x Socket API Controller                                 │
│    - Cgroup v2 Quota & Resource Throttling (CPU, RAM)                       │
│    - TAP Interface & IPAM Network Management                                │
│    - Dynamic Scale-to-Zero GPU VRAM Pool Manager                            │
│    - Shared Storage Volume Manager (RWO / RWX)                              │
│    - Per-second Metered Credit Billing Engine                               │
└──────────────────────┬──────────────────────────────────────────────────────┘
                       │ Local Disk (ext4) & Socket
                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│ 3. mos-builder (Zero-Config Build Engine)                                   │
│    - Embedded Rust Nixpacks Analysis Engine                                 │
│    - Language & Framework Auto-Detection (Node, Python, Rust, Go, etc.)     │
│    - Minimal ext4 Rootfs Image Packaging                                    │
│    - Litestream / SQLite Automated Streaming Replication                    │
└─────────────────────────────────────────────────────────────────────────────┘
                       │
         ┌─────────────┴───────────────────────────┐
         ▼                                         ▼
┌─────────────────────────────────┐       ┌─────────────────────────────────┐
│ Firecracker MicroVM #1          │       │ Firecracker MicroVM #2          │
│ - Guest Kernel: vmlinux         │       │ - Guest Kernel: vmlinux         │
│ - Rootfs: Alpine / ext4         │       │ - Rootfs: Alpine / ext4         │
│ - User App: Next.js 14 SSR      │       │ - User App: FastAPI + SQLite    │
│ - Guest Init: mos-init (PID 1)  │       │ - Guest Init: mos-init (PID 1)  │
│ - TAP IP: 172.16.0.2            │       │ - TAP IP: 172.16.0.3            │
└─────────────────────────────────┘       └─────────────────────────────────┘
```

---

## 3. Core Component Specifications

### 3.1. `mos-edge` (Ingress Proxy & Traffic Director)
* **Role**: Receives client HTTP/WebSocket traffic, inspects routing state, buffers requests if the target MicroVM is suspended, and forwards traffic immediately upon wake-up.
* **Key Capabilities**:
  * **Sub-millisecond Routing Table**: Maps domain names (`app.example.com`) to instance state (`Running`, `Suspended`, `Stopped`, `Building`) and target IPs.
  * **TCP/HTTP Request Buffering**: Buffers incoming request headers and payloads in memory while triggering Orchestrator wake signals.
  * **Automated TLS & ACME**: Automates Let's Encrypt certificates with HTTP-01 challenge handling and SNI resolution.
  * **3-Stage Weighted Canary**: Manages progressive rollouts (`10% -> 50% -> 100%`) with automatic error detection and instant HMAC rollbacks.

### 3.2. `mos-orchestrator` (Host Node Daemon)
* **Role**: Supervises local Firecracker MicroVM processes, manages hardware virtualization resources, and orchestrates scale-to-zero snapshots.
* **Key Capabilities**:
  * **Firecracker Socket Controller**: Communicates over Unix Domain Sockets (`/boot-source`, `/drives`, `/network-interfaces`, `/vsock`, `/actions`, `/snapshot/create`, `/snapshot/load`).
  * **Userfaultfd (UFFD) Engine**: Enables on-demand lazy memory paging for sub-millisecond (1.20 ms) MicroVM resume times with Zstandard compression.
  * **Dynamic GPU VRAM Pooling**: Dynamically binds GPU VRAM for LLM inference workloads and releases VRAM to 0 MB when instances idle.
  * **Cgroups v2 & TAP IPAM**: Enforces hard CPU/memory limits and provisions isolated `tap-mos-XXX` network devices with NAT port forwarding.

### 3.3. `mos-builder` (Zero-Config Build Pipeline)
* **Role**: Analyzes source code repositories and packages them into executable ext4 root filesystem images.
* **Pipeline Steps**:
  1. **Source Inspection**: Generates a Nixpacks build plan (runtime provider, dependencies, build command, entrypoint).
  2. **Artifact Compilation**: Builds user code in an isolated temporary environment.
  3. **Rootfs Overlay**: Packages compiled binaries onto a minimal base image containing the `mos-init` guest shim.
  4. **Database Configuration**: Detects SQLite databases and automatically injects `litestream.yml` for S3/R2 replication.

### 3.4. `mos-init` (Static Guest Supervisor)
* **Role**: Runs as PID 1 (`init`) inside the MicroVM guest kernel.
* **Binary Size**: <820 KB statically compiled Rust binary.
* **Execution Flow**:
  * Mounts `/proc`, `/sys`, `/dev`, `/run`, and `/tmp` in <1.15 ms.
  * Brings up loopback and `eth0` network interfaces.
  * Spawns user application and captures stdout/stderr streams.
  * Reaps zombie child processes and communicates telemetry over AF_VSOCK channels.

### 3.5. `mos-cluster` (P2P Mesh Network)
* **Role**: Provides decentralized multi-node coordination.
* **Key Capabilities**:
  * **SWIM Gossip Protocol**: Node membership discovery, heartbeats, and failure detection.
  * **Consistent Hash Ring**: Deterministic request routing and failover across cluster nodes.

---

## 4. Scale-to-Zero State Machine

```
              ┌──────────────┐
              │   BUILDING   │
              └──────┬───────┘
                     │ Build completed & Rootfs ready
                     ▼
              ┌──────────────┐
              │   STARTING   │
              └──────┬───────┘
                     │ MicroVM booted & Healthcheck OK (<10ms)
                     ▼
       ┌──────▶┌──────────────┐◀─────┐
       │       │   RUNNING    │      │
       │       └──────┬───────┘      │
       │              │              │
Wake-on-HTTP (<7ms)   │ 300s Idle    │ Fast Resume (<2ms)
       │              ▼              │
       │       ┌──────────────┐      │
       └───────┤  SUSPENDED   ├──────┘
               │ (0MB Memory) │
               └──────┬───────┘
                      │ mos stop / Operator termination
                      ▼
               ┌──────────────┐
               │   STOPPED    │
               └──────────────┘
```
