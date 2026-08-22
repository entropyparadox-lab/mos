<div align="center">

# 🦀 MOS (MicroVM Operating Service)

**Linux KVMおよびFirecracker MicroVMを基盤とする、超軽量・超高密度なScale-to-Zero Serverless PaaS**

[![CI](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml/badge.svg)](https://github.com/entropyparadox-lab/mos/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Linux%20x86__64%20(KVM)-lightgrey.svg)](https://kernel.org)
[![Latency](https://img.shields.io/badge/Wake--on--HTTP-%3C%207.00ms-success.svg)](docs/BENCHMARK_REPORT.md)

**[ English ](README.md)** • **[ 한국어 ](README.ko.md)** • **[ 日本語 ](README.ja.md)** • **[ 简体中文 ](README.zh.md)**

[概要](#概要) • [主な機能](#-主な機能) • [ベンチマーク実測値](#-ベンチマーク実測値) • [アーキテクチャ](#%EF%B8%8F-アーキテクチャ) • [クイックスタート](#-クイックスタート) • [設定ガイド](#%EF%B8%8F-設定ガイド-mostoml) • [ドキュメント](docs/)

</div>

---

## 概要

**MOS (MicroVM Operating Service)** は、バイブコーダーおよびモダンなクラウドワークロードのために開発された、Rustネイティブなサーバーレスランタイム＆エッジホスティングプラットフォームです。

`Dockerfile` や複雑なKubernetesマニフェストを記述することなく、ソースコードから数秒でビルド＆デプロイを完了できます。従来のコンテナによるカーネル共有方式の脆弱性を排除し、**Linux KVM + AWS Firecracker MicroVM** によるハードウェアレベルの仮想化分離境界を保証します。さらに、アイドル時にはメモリをスナップショット状態に退避させ、**0 MB RAMおよび0 MB GPU VRAM** の完全なScale-to-Zeroを実現します。

サスペンド状態のインスタンスにHTTPリクエストが到達すると、MOSは **1.20 ms（Userfaultfd遅延ページング）** または **6.57 ms（フルスナップショット復元）** 以内にMicroVMを即座に起動し、パケットを一切ドロップすることなく7ms以内で応答を配信します。

---

## 🌟 主な機能

| 機能 | 説明 |
| :--- | :--- |
| ⚡ **Sub-7ms Wake-on-HTTP** | アイドル時は完全な0 MBフットプリント。HTTP要求を受信すると **1.20 ms (UFFD遅延ページング)** / **6.57 ms (スナップショット復元)** 以内に即座に復帰 |
| 🔒 **ハードウェアレベルの分離** | Linux KVM + Firecracker によるハードウェア仮想化境界により、コンテナ脱獄やカーネル共有のリスクを排除 |
| 🚀 **設定不要のビルド＆デプロイ (Zero-Config)** | Rust Nixpacks エンジンを内蔵。Node.js、Python、Rust、Go を自動検出して最小限の ext4 Rootfs を生成 |
| 💾 **SQLite-First & Litestream** | SQLite データベースを自動検知し、S3 や Cloudflare R2 へのトランザクションリアルタイムストリーミングバックアップを自動化 |
| 🌐 **超高速エッジイングレス** | Hyper/Tokio による非同期リバースプロキシ、ACME/TLS自動化、3段階カナリアデプロイ (`10% -> 50% -> 100%`)、HMAC瞬時ロールバック |
| 🛰️ **分散P2Pメッシュクラスタ** | SWIM Gossip プロトコルおよび Consistent Hash Ring による自律分散ノードディスカバリ＆クロスノードルーティング |
| 🎯 **動的Scale-to-Zero GPUプール** | AI/LLM推論ワークロードに対してGPU VRAMを動的に割り当て、アイドル時には 0 MB へ即座に解放 |
| 🛡️ **eBPF/XDP防御 & Ed25519 RBAC** | カーネルレベルでのL4 DDoS防御フィルターと、非対称暗号署名に基づくステートレスRBACトークン検証 (<0.01 ms) |
| 📊 **リアルタイム計量＆クレジット請求** | vCPU、RAM、VRAM、エグレスの秒単位精密計量と残高枯渇時の自動サスペンド |

---

## 📊 ベンチマーク実測値

物理ベアメタルサーバー（Linux 6.17, AMD Ryzen 7 9700X 8C/16T, KVM, NVMe SSD）における公認実測値です（[詳細ベンチマークレポート](docs/BENCHMARK_REPORT.md)）。

```
┌───────────────────────────────────────────────┬─────────────────┬────────────────────────┐
│ 測定項目                                      │ 実測値          │ 従来コンテナ/Lambda比較│
├───────────────────────────────────────────────┼─────────────────┼────────────────────────┤
│ MicroVM Cold Boot (KVM Kernel Init)           │ 10.06 ms        │ 従来のコンテナより50-100倍高速 │
│ Guest PID 1 `mos-init` 初期起動               │ 1.15 ms         │ /proc, /sys, eth0 UP   │
│ Scale-to-Zero メモリスナップショット (128MB)  │ 123.97 ms       │ Full Memory Dump       │
│ Fast Snapshot Resume (Full Memory Map)        │ 6.57 ms         │ AWS Lambdaより20倍高速 │
│ UFFD On-Demand Lazy Resume (Zstandard)        │ 1.20 ms         │ 1ms台の超高速復帰      │
│ End-to-End Wake-on-HTTP レイテンシ            │ < 7.00 ms       │ バッファリング -> 復帰 -> 応答 │
│ Ed25519 RBAC ステートレストークン検証         │ < 0.01 ms       │ 毎秒10万回以上の処理能力│
│ eBPF XDP パケットフィルターオーバーヘッド     │ < 0.02 ms       │ カーネルL4パケット破棄 │
│ Consistent Hash Ring ノードルーティング照会   │ < 0.05 ms       │ O(log N) 二分探索      │
│ Scale-to-Zero アイドル占有量 (RAM / VRAM)     │ 0.0 MB          │ リソースの無駄ゼロ     │
└───────────────────────────────────────────────┴─────────────────┴────────────────────────┘
```

---

## 🏗️ アーキテクチャ

MOSは7つの独立したRustクレートによるモジュール型ワークスペースとして構築されています。

```
                     [ Public Traffic / Clients ]
                                  │
                                  ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🌐 mos-edge (超高性能イングレスプロキシ)                    │
   │    • eBPF / XDP DDoS フィルター • W3C 分散トレーシング伝播   │
   │    • TCP/HTTP 要求バッファリング • Wake-on-HTTP IPC トリガー │
   │    • 自動 TLS (ACME)             • 3段階カナリア展開パイプライン│
   └──────────────┬───────────────────────────────┬──────────────┘
                  │ UDS / IPC Signal              │ Direct Proxy
                  ▼                               ▼
   ┌───────────────────────────────┐  ┌──────────────────────────┐
   │ 🛰️ mos-cluster                │  │ 🏗️ mos-builder           │
   │    • SWIM Gossip ノード検知   │  │    • Nixpacks 自動分析   │
   │    • Consistent Hash Ring     │  │    • ext4 Rootfs 生成    │
   │    • グローバルクロスルーティング│ │    • Litestream 複製注入 │
   └──────────────┬────────────────┘  └───────────┬──────────────┘
                  │                               │
                  ▼                               ▼
   ┌─────────────────────────────────────────────────────────────┐
   │ 🎛️ mos-orchestrator (ホストコントローラー & ノードデーモン)  │
   │    • Firecracker v1.x Socket API コントローラー             │
   │    • メモリスナップショット & UFFD 遅延ページングエンジン    │
   │    • 動的 GPU VRAM プール管理                               │
   │    • Cgroup v2 クォータおよび TAP/IPAM ネットワーク管理     │
   │    • 共有 RWO/RWX ボリューム管理＆計量請求エンジン          │
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

## 🚀 クイックスタート

### 前提条件

* **Linux OS** (Ubuntu 22.04+, Debian 12+, Arch Linux など)
* **KVM有効化**: `/dev/kvm` へのアクセス権限
* **Cgroups v2** 有効化
* **Rust Toolchain**: 1.80+ (`rustup default stable`)

### 1. インストールとビルド

```bash
git clone https://github.com/entropyparadox-lab/mos.git
cd mos
cargo build --release
sudo ln -sf $(pwd)/target/release/mos /usr/local/bin/mos
```

### 2. ホスト事前チェックと初期化

```bash
mos host preflight
sudo mos host init --dir /var/lib/mos
```

### 3. アプリケーションのデプロイ

```bash
# Next.js, FastAPI, Rust Axum アプリを即座にデプロイ
mos deploy ./examples/vibe-nextjs-app

# 起動インスタンス一覧の確認
mos list
```

### 4. エッジプロキシの起動

```bash
# デフォルトまたは設定ファイルによる起動 (詳細: docs/INGRESS_ROUTING.md)
mos edge --port 8180 --upstream 127.0.0.1:8080 --domain myapp.local
```

### 5. Webダッシュボードの起動

```bash
mos dashboard --port 8080
# ブラウザで http://localhost:8080 を開く
```

---

## ⚙️ 設定ガイド (`mos.toml`)

MOSは **デフォルトでZero-Config（設定ファイル不要）** で動作します。リソース拡張や独自ドメイン、GPU VRAM割り当てをカスタマイズする場合のみ、プロジェクトルートに `mos.toml` を配置します（[詳細設定仕様書](docs/MOS_CONFIG_SPEC.md)）。

```toml
# mos.toml (オプション)
[app]
name = "my-service"

[resources]
vcpu = 2
memory_mib = 512
# gpu_vram_mib = 8192                # AI推論用 GPU VRAM 割り当て

[network]
port = 3000
domain = "my-service.mos.local"
egress = "allow-all"                 # "allow-all" または "whitelist-only"

[storage.litestream]
enabled = true                       # SQLite S3/Cloudflare R2 リアルタイム同期
db_path = "app.db"
replica_type = "s3"
bucket = "my-app-db-replicas"

[scaling]
idle_timeout_seconds = 300
strategy = "uffd"                    # 1.2ms UFFD 遅延復帰
```

---

## 📄 ライセンス

MOSは利用者の選択により、以下のデュアルライセンスの下で提供されます：

* **MIT License** ([LICENSE-MIT](LICENSE-MIT))
* **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE))
