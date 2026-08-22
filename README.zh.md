<div align="center">

# 🦀 MOS (MicroVM Operating Service)

**基于 Linux KVM 和 Firecracker MicroVM 的超轻量、超高密度 Scale-to-Zero Serverless PaaS 平台**

[![CI](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml/badge.svg)](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64%20(KVM)-lightgrey.svg)](https://kernel.org)
[![Latency](https://img.shields.io/badge/Wake--on--HTTP-%3C%207.00ms-success.svg)](docs/BENCHMARK_REPORT.md)

**[ English ](README.md)** • **[ 한국어 ](README.ko.md)** • **[ 日本語 ](README.ja.md)** • **[ 简体中文 ](README.zh.md)**

[概述](#概述) • [核心特性](#-核心特性) • [实测性能基准](#-实测性能基准) • [系统架构](#%EF%B8%8F-系统架构) • [快速开始](#-快速开始-quickstart) • [配置指南](#%EF%B8%8F-配置指南-mostoml) • [文档中心](docs/)

</div>

---

## 概述

**MOS (MicroVM Operating Service)** 是专为 Vibe Coder 和现代云原生高并发工作负载打造的 Rust 原生无服务器运行时与边缘托管平台。

无需编写 `Dockerfile` 或复杂的 Kubernetes YAML，仅需源代码即可在数秒内完成自动构建与极速部署。MOS 彻底打破传统容器共享内核的安全瓶颈，通过 **Linux KVM + AWS Firecracker MicroVM** 提供硬件级别的强隔离边界。在实例空闲时自动休眠为内存快照，实现 **0 MB RAM 和 0 MB GPU VRAM** 的真正 Scale-to-Zero。

当 HTTP 流量到达处于休眠状态的实例时，MOS 会在 **1.20 ms（Userfaultfd 惰性分页）** 或 **6.57 ms（全内存快照恢复）** 内即时唤醒 MicroVM，在 7ms 内零丢包转发请求并返回响应。

---

## 🌟 核心特性

| 特性 | 描述 |
| :--- | :--- |
| ⚡ **Sub-7ms Wake-on-HTTP** | 空闲时内存完全清零 (0 MB)；HTTP 请求到达时 **1.20 ms (UFFD 惰性分页)** / **6.57 ms (快照恢复)** 内极速唤醒并转发 |
| 🔒 **硬件级安全隔离** | Linux KVM 硬件虚拟化 + Firecracker 边界，杜绝容器逃逸与内核漏洞风险 |
| 🚀 **零配置构建与部署 (Zero-Config)** | 内置 Rust Nixpacks 引擎 — 自动识别 Node.js、Python、Rust、Go 等技术栈，免 Dockerfile 构建 ext4 Rootfs |
| 💾 **SQLite-First & Litestream** | 自动探测 SQLite 数据库，实现事务流实时同步备份至 S3 及 Cloudflare R2 |
| 🌐 **超低延迟边缘网关** | 基于 Hyper/Tokio 的高性能异步反向代理，自动 ACME/TLS 证书管理，三阶段加权金丝雀发布 (`10% -> 50% -> 100%`) 与 HMAC 即时回滚 |
| 🛰️ **去中心化 P2P 节点集群** | 基于 SWIM Gossip 协议与一致性哈希环 (Consistent Hash Ring) 的自组织节点发现与全局跨节点路由 |
| 🎯 **动态 Scale-to-Zero GPU 池** | 针对 AI/LLM 推理工作负载动态分配 GPU 显存，空闲时 0 MB 瞬时释放 |
| 🛡️ **eBPF/XDP 内核级防护 & Ed25519 RBAC** | 内核层 L4 DDoS 报文过滤 + 基于非对称加密签名的无状态 RBAC 鉴权 (<0.01 ms) |
| 📊 **实时计量与按量计费引擎** | vCPU、内存、显存及外网流量秒级精密计量，余额耗尽自动休眠 |

---

## 📊 实测性能基准

在物理裸金属服务器（Linux 6.17, AMD Ryzen 7 9700X 8C/16T, KVM, NVMe SSD）上的实测公认数据（详见 [完整基准测试报告](docs/BENCHMARK_REPORT.md)）：

```
┌───────────────────────────────────────────────┬─────────────────┬────────────────────────┐
│ 测试指标                                      │ 实测延迟        │ 对比基准性能           │
├───────────────────────────────────────────────┼─────────────────┼────────────────────────┤
│ MicroVM Cold Boot (KVM Kernel Init)           │ 10.06 ms        │ 比传统容器快 50-100 倍 │
│ Guest PID 1 `mos-init` 早期引导               │ 1.15 ms         │ 完成 /proc, /sys, eth0 挂载 │
│ Scale-to-Zero 内存快照创建 (128MB)            │ 123.97 ms       │ Full Memory Dump       │
│ Fast Snapshot Resume (全内存映射恢复)         │ 6.57 ms         │ 比 AWS Lambda 快 20 倍 │
│ UFFD On-Demand Lazy Resume (Zstandard)        │ 1.20 ms         │ 1ms 级极速唤醒         │
│ 端到端 Wake-on-HTTP 往返延迟                  │ < 7.00 ms       │ 请求缓冲 -> 唤醒 -> 响应│
│ Ed25519 RBAC 无状态令牌验签                  │ < 0.01 ms       │ 吞吐量超 100,000 ops/s │
│ eBPF XDP 报文过滤开销                         │ < 0.02 ms       │ 内核 L4 极速放行/丢弃  │
│ Consistent Hash Ring 节点路由查找             │ < 0.05 ms       │ O(log N) 二分检索      │
│ Scale-to-Zero 空闲资源占用 (RAM / VRAM)       │ 0.0 MB          │ 真正的零资源消耗       │
└───────────────────────────────────────────────┴─────────────────┴────────────────────────┘
```

---

## 🏗️ 系统架构

MOS 由 7 个高度模块化的 Rust Crate 组成。

```
                     [ Public Traffic / Clients ]
                                  │
                                  ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🌐 mos-edge (超高性能边缘反向代理)                          │
   │    • eBPF / XDP DDoS 过滤器  • W3C 分布式链路追踪传播       │
   │    • TCP/HTTP 请求缓冲       • Wake-on-HTTP IPC 唤醒触发    │
   │    • 自动 TLS (ACME)         • 三阶段渐进式金丝雀发布流水线 │
   └──────────────┬───────────────────────────────┬──────────────┘
                  │ UDS / IPC 信号                │ Direct Proxy
                  ▼                               ▼
   ┌───────────────────────────────┐  ┌──────────────────────────┐
   │ 🛰️ mos-cluster                │  │ 🏗️ mos-builder           │
   │    • SWIM Gossip 成员协议     │  │    • Nixpacks 自动分析   │
   │    • 一致性哈希环             │  │    • ext4 Rootfs 镜像打包│
   │    • 全局跨节点路由决策       │  │    • Litestream 实时复制 │
   └──────────────┬────────────────┘  └───────────┬──────────────┘
                  │                               │
                  ▼                               ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🎛️ mos-orchestrator (主机控制器与节点守护进程)              │
   │    • Firecracker v1.x Socket API 控制器                     │
   │    • 内存快照与 UFFD 惰性分页恢复引擎                       │
   │    • 动态 GPU 显存池管理器                                  │
   │    • Cgroup v2 资源配额与 TAP/IPAM 网络管理                 │
   │    • 共享 RWO/RWX 存储卷管理与实时计量计费引擎              │
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

---

## 🚀 快速开始 (Quickstart)

### 环境要求

* **Linux 操作系统** (Ubuntu 22.04+, Debian 12+, Arch Linux 等)
* **开启 KVM**: 具备 `/dev/kvm` 读写权限
* **开启 Cgroups v2**
* **Rust 工具链**: 1.80+ (`rustup default stable`)

### 1. 安装与编译

```bash
git clone https://github.com/entropyparadox-lab/mos.git
cd mos
cargo build --release
sudo ln -sf $(pwd)/target/release/mos /usr/local/bin/mos
```

### 2. 宿主机环境预检与初始化

```bash
mos host preflight
sudo mos host init --dir /var/lib/mos
```

### 3. 部署应用程序

```bash
# 自动探测并部署 Next.js、FastAPI 或 Rust Axum 应用
mos deploy ./examples/vibe-nextjs-app

# 查看运行中的 MicroVM 实例
mos list
```

### 4. 启动边缘反向代理

```bash
# 启动默认网关或加载静态路由表 (详见 docs/INGRESS_ROUTING.md)
mos edge --port 8180 --upstream 127.0.0.1:8080 --domain myapp.local
```

### 5. 启动 Web 控制台

```bash
mos dashboard --port 8080
# 在浏览器中打开 http://localhost:8080
```

---

## ⚙️ 配置指南 (`mos.toml`)

MOS **默认以 Zero-Config 模式运行**。仅在需要自定义资源配额、绑定域名、分配 GPU 显存或配置网络白名单时，才需在项目根目录创建 `mos.toml`（详见 [配置规范全文](docs/MOS_CONFIG_SPEC.md)）。

```toml
# mos.toml (可选)
[app]
name = "my-service"

[resources]
vcpu = 2
memory_mib = 512
# gpu_vram_mib = 8192                # AI 推理 GPU 显存分配

[network]
port = 3000
domain = "my-service.mos.local"
egress = "allow-all"                 # "allow-all" 或 "whitelist-only"

[storage.litestream]
enabled = true                       # SQLite S3/Cloudflare R2 实时同步
db_path = "app.db"
replica_type = "s3"
bucket = "my-app-db-replicas"

[scaling]
idle_timeout_seconds = 300
strategy = "uffd"                    # 1.2ms UFFD 惰性恢复
```

---

## 📄 开源许可证

MOS 支持双重开源许可证，用户可自由选择：

* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
