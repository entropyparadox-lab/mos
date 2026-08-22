# 📋 MOS Technical Implementation Specification (SPEC.md)

---

## 1. Workspace Crate Architecture

```
mos/
├── Cargo.toml                  # Workspace Root Manifest (7 crates)
├── docs/
│   ├── ARCHITECTURE.md         # Global architecture & state machine
│   ├── SPEC.md                 # Technical specification & API interfaces
│   ├── BENCHMARK_REPORT.md     # Production GA performance benchmark report
│   ├── MOS_CONFIG_SPEC.md      # mos.toml configuration specification
│   └── INGRESS_ROUTING.md      # Edge ingress proxy & routing guide
├── crates/
│   ├── mos-core/               # Domain models, state machine, RBAC & billing
│   ├── mos-orchestrator/       # Firecracker lifecycle, TAP IPAM, cgroups v2, GPU pool
│   ├── mos-edge/               # Hyper ingress proxy, request buffering, ACME/TLS
│   ├── mos-builder/            # Nixpacks engine, ext4 rootfs builder, Litestream
│   ├── mos-init/               # Static PID 1 guest supervisor binary (<820 KB)
│   ├── mos-cluster/            # SWIM Gossip mesh & consistent hash ring
│   └── mos-cli/                # Operator CLI tool & Web Dashboard
├── examples/                   # Reference applications (Next.js, FastAPI, Axum)
├── scripts/                    # Asset setup & standalone verification scripts
└── mos.example.toml            # Project configuration template
```

---

## 2. Crate Dependencies & Responsibilities

| Crate | Key Dependencies | Primary Purpose |
| :--- | :--- | :--- |
| **`mos-core`** | `serde`, `serde_json`, `thiserror`, `tokio`, `uuid`, `ed25519-dalek` | Domain entities, RBAC token verification, credit billing engine |
| **`mos-orchestrator`** | `hyper-util`, `hyper`, `tokio`, `nix`, `tracing`, `zstd` | Firecracker socket IPC, UFFD lazy paging, cgroups v2, GPU VRAM pool |
| **`mos-edge`** | `hyper`, `tokio`, `rustls`, `bytes`, `dashmap`, `hmac`, `sha2` | Reverse proxy, request buffering, ACME HTTP-01 TLS, 3-stage canary |
| **`mos-builder`** | `nixpacks`, `tokio-process`, `tempfile` | Source code analysis, ext4 rootfs builder, Litestream injection |
| **`mos-init`** | `nix`, `libc`, `tokio` | Static PID 1 guest init binary, VFS mounting, vsock telemetry |
| **`mos-cluster`** | `tokio`, `serde`, `rand` | Decentralized SWIM Gossip clustering, consistent hash ring routing |
| **`mos-cli`** | `clap`, `axum`, `reqwest`, `tokio` | Operator CLI, host provisioning wizard, live web dashboard |

---

## 3. Network & IPAM Specifications

* **Host MicroVM Subnet**: `172.16.0.0/16` (Up to 65,534 isolated MicroVMs per node)
* **Host Gateway Bridge**: `mos-br0` (`172.16.0.1`)
* **Per-Instance TAP Devices**: `tap-mos-001`, `tap-mos-002`, ...
* **Guest IP Assignment**:
  * VM #1: `172.16.0.2`
  * VM #2: `172.16.0.3`
  * Gateway: `172.16.0.1`
  * DNS: `1.1.1.1` (Cloudflare) / `8.8.8.8` (Google)
* **NAT / Egress Forwarding**:
  `iptables -t nat -A POSTROUTING -s 172.16.0.0/16 ! -d 172.16.0.0/16 -j MASQUERADE`

---

## 4. Firecracker JSON API Contract Examples

```json
// 1. Boot Source Configuration
PUT /boot-source
{
  "kernel_image_path": "/var/lib/mos/kernels/vmlinux.bin",
  "boot_args": "console=ttyS0 reboot=k panic=1 pci=off init=/init ip=172.16.0.2::172.16.0.1:255.255.0.0::eth0:off"
}

// 2. Root Drive Configuration
PUT /drives/rootfs
{
  "drive_id": "rootfs",
  "path_on_host": "/var/lib/mos/instances/inst-01/rootfs.ext4",
  "is_root_device": true,
  "is_read_only": false
}

// 3. Network Interface Configuration
PUT /network-interfaces/eth0
{
  "iface_id": "eth0",
  "guest_mac": "AA:FC:00:00:00:02",
  "host_dev_name": "tap-mos-001"
}

// 4. Machine Configuration (vCPU / Memory)
PUT /machine-config
{
  "vcpu_count": 1,
  "mem_size_mib": 256,
  "smt": false
}

// 5. Instance Start Action
PUT /actions
{
  "action_type": "InstanceStart"
}
```

---

## 5. Scale-to-Zero Snapshot Specification

1. **Pause & Create Snapshot**:
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
2. **Resume from Snapshot**:
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

## 6. Guest Supervisor (`mos-init`) Protocol

1. **Early Mount Phase**:
   * Mount `/proc`, `/sys`, `/dev` (devtmpfs), `/run` (tmpfs), and `/tmp` (tmpfs).
2. **Network Link Setup**:
   * Configure loopback `lo` UP.
   * Assign static IP or spawn background `udhcpc` on `eth0`.
3. **Application Supervisor**:
   * Execute application entrypoint with environment variables.
   * Stream stdout/stderr over AF_VSOCK (Port `5252`).
   * Handle signals (`SIGTERM`, `SIGKILL`) and reap zombie child processes.
