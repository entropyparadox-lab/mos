# ⚙️ `mos.toml` Configuration Specification & Guide

> **MOS (MicroVM Operating Service) Application Configuration Specification**  
> `mos.toml` is an optional project-level manifest used to customize hardware virtualization resources, ingress/egress networking, persistent storage, and scale-to-zero lifecycle policies.

---

## 📌 Core Principles: Zero-Config & Progressive Override

1. **Zero-Config Default**:
   * If `mos.toml` is omitted, `mos deploy` automatically detects your language runtime (Node.js, Python, Rust, Go, etc.) using Nixpacks and deploys with default production specifications (1 vCPU, 128 MB RAM, full outbound internet access).
2. **Progressive Override**:
   * You only need to define sections and keys that differ from the defaults (e.g., allocating GPU VRAM, increasing memory, attaching shared storage, or enforcing strict egress firewall whitelists).

---

## 📑 Full Configuration Schema

```toml
# ==============================================================================
# 1. Application Metadata [app]
# ==============================================================================
[app]
name = "my-service"                  # Application identifier (defaults to folder name)
version = "1.0.0"                    # Application version tag
provider = "node"                    # Runtime override ("node" | "python" | "rust" | "go" | "static")
build_command = "npm run build"      # Custom build command override (auto-detected if omitted)
start_command = "npm run start"      # Custom start command override (auto-detected if omitted)

# ==============================================================================
# 2. Hardware Resource Quotas [resources]
# ==============================================================================
[resources]
vcpu = 1                             # Virtual CPU cores (default: 1)
memory_mib = 256                     # RAM in MiB (default: 128)
# gpu_vram_mib = 8192                # Scale-to-Zero GPU VRAM allocation in MiB (for AI/LLM models)

# ==============================================================================
# 3. Networking & Ingress/Egress Firewall [network]
# ==============================================================================
[network]
port = 3000                          # Guest application listening port (auto-detected if omitted)
domain = "my-service.mos.local"      # Primary ingress routing domain
tls = "auto"                         # TLS certificate mode ("auto" | "self-signed" | "off")

# Outbound (Egress) Firewall Policy:
#  - "allow-all": Unrestricted outbound internet connectivity (Default - GA, Sentry, external APIs)
#  - "whitelist-only": Drops all external traffic except endpoints in `allowed_outbound`
egress = "allow-all"

# Whitelisted external FQDNs / endpoints (when egress = "whitelist-only")
allowed_outbound = [
    "o12345.ingest.sentry.io",
    "www.google-analytics.com",
    "generativelanguage.googleapis.com",
    "api.openai.com",
    "api.anthropic.com"
]

# ==============================================================================
# 4. Storage & SQLite Litestream Streaming [storage]
# ==============================================================================
[storage]
# Distributed shared volume mounts (optional)
# volumes = [
#     { name = "shared-uploads", mount_path = "/app/uploads", mode = "rwx" }
# ]

# SQLite real-time S3 / Cloudflare R2 replication streaming
[storage.litestream]
enabled = true                       # Auto-enabled when SQLite database file is detected
db_path = "data/app.db"              # SQLite database file path
replica_type = "s3"                  # Storage backend ("s3" | "gcs" | "abs")
bucket = "my-app-db-replicas"        # S3 / Cloudflare R2 bucket name
# s3_endpoint = "https://<account-id>.r2.cloudflarestorage.com"

# ==============================================================================
# 5. Scale-to-Zero & Lifecycle Scaling [scaling]
# ==============================================================================
[scaling]
idle_timeout_seconds = 300           # Inactivity threshold before memory snapshot (seconds, default: 300)
strategy = "uffd"                    # Restore acceleration strategy ("uffd" [1.2ms lazy paging] | "snapshot" [6.5ms full])
min_instances = 0                    # Minimum instances (0 = True Scale-to-Zero)
max_instances = 10                   # Maximum autoscaling ceiling

# ==============================================================================
# 6. Progressive Canary Deployment [canary]
# ==============================================================================
[canary]
enabled = false                      # Enable automated canary traffic promotion
initial_weight = 10                  # Initial canary traffic allocation (10%)
step_weights = [10, 50, 100]         # Progressive rollout steps (10% -> 50% -> 100%)
step_interval_seconds = 60           # Evaluation interval between promotion steps
error_threshold_pct = 1.0            # 5xx error threshold percentage (triggers instant rollback)

# ==============================================================================
# 7. Environment Variables [env]
# ==============================================================================
[env]
NODE_ENV = "production"
PORT = "3000"
LOG_LEVEL = "info"
```

---

## 🎯 Production Recipes

### Recipe 1: Next.js 14 SSR Application (with Sentry & Analytics)

```toml
[app]
name = "nextjs-storefront"

[resources]
vcpu = 2
memory_mib = 512

[network]
port = 3000
domain = "store.example.com"
tls = "auto"
egress = "allow-all" # Sentry & Google Analytics telemetry

[env]
NODE_ENV = "production"
NEXT_TELEMETRY_DISABLED = "1"
```

---

### Recipe 2: FastAPI + SQLite + Litestream S3 Streaming Backup

```toml
[app]
name = "fastapi-backend"

[resources]
vcpu = 1
memory_mib = 256

[network]
port = 8000
domain = "api.example.com"

[storage.litestream]
enabled = true
db_path = "app.db"
replica_type = "s3"
bucket = "my-fastapi-db-backups"

[scaling]
idle_timeout_seconds = 180
strategy = "uffd" # 1.20ms sub-millisecond resume
```

---

### Recipe 3: Scale-to-Zero GPU LLM Inference Server (0 MB Idle VRAM)

```toml
[app]
name = "llama3-inference-service"

[resources]
vcpu = 4
memory_mib = 4096
gpu_vram_mib = 16384 # 16 GB dynamic GPU VRAM allocation

[network]
port = 8000
domain = "ai.example.com"

[scaling]
idle_timeout_seconds = 60 # Release GPU VRAM to 0 MB after 60s idle
strategy = "uffd"
```

---

### Recipe 4: Enterprise Strict Security Backend (Whitelist Egress)

```toml
[app]
name = "fintech-settlement-engine"

[resources]
vcpu = 2
memory_mib = 1024

[network]
port = 8080
domain = "settle.internal.local"
egress = "whitelist-only" # Block all unauthorized outbound network traffic
allowed_outbound = [
    "api.payment-gateway.com",
    "o99999.ingest.sentry.io"
]
```
