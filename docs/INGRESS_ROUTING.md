# 🌐 MOS Ingress & Edge Routing Guide

> **MOS (MicroVM Operating Service) Ingress Proxy & Domain Routing Guide**  
> `mos-edge` is a high-throughput, sub-millisecond reverse proxy designed to route public traffic across thousands of isolated MicroVMs and upstream services from a unified entrypoint (Ports 80, 443, 8180).

---

## 📌 Dual Routing Architecture

MOS provides two complementary routing models tailored for development agility and declarative operations.

```
                                [ Ingress Traffic (*.example.com) ]
                                                │
                                                ▼
                        ┌───────────────────────────────────────────────┐
                        │ 🌐 mos-edge (High-Performance Ingress Proxy)   │
                        │    - Sub-millisecond Hash Table Routing       │
                        │    - TCP/HTTP Request Buffering               │
                        │    - Wake-on-HTTP IPC Signal Trigger          │
                        └───────────────┬───────────────┬───────────────┘
                                        │               │
            ┌───────────────────────────┘               └───────────────────────────┐
            ▼                                                                       ▼
┌───────────────────────────────────────┐                       ┌───────────────────────────────────────┐
│ 1. Dynamic MicroVM Ingress (CLI)      │                       │ 2. Static Gateway Routing (Config)    │
│  - Triggered during `mos deploy ./app`│                       │  - Loaded from `config/routes.json`   │
│  - Registered into in-memory table    │                       │  - Declarative multi-domain routing   │
│  - Auto-updates IP on wake-up         │                       │  - Replaces Nginx / Caddy gateways    │
└───────────────────────────────────────┘                       └───────────────────────────────────────┘
```

---

## 1. Static Gateway Routing (`config/routes.json`)

Similar to `nginx.conf` or `Caddyfile`, operators use `config/routes.json` to declaratively map multiple external subdomains to internal services and ports.

### Setup Instructions
```bash
# 1. Copy the example template to create your local routes.json (ignored by Git)
cp config/routes.example.json config/routes.json

# 2. Define your domains and upstream targets
```

### `config/routes.json` Schema Specification
```json
{
  "routes": {
    "app.example.com": {
      "host": "127.0.0.1",
      "port": 8080,
      "is_suspended": false
    },
    "api.example.com": {
      "host": "127.0.0.1",
      "port": 8000,
      "is_suspended": false
    },
    "worker.example.com": {
      "host": "127.0.0.1",
      "port": 8085,
      "is_suspended": true
    }
  }
}
```

* **`host`**: Destination IP (e.g. `127.0.0.1` or MicroVM TAP IP `172.16.x.x`)
* **`port`**: Upstream listening TCP port
* **`is_suspended`**:
  * `false`: Always-on active service (direct immediate reverse proxy forwarding)
  * `true`: **Scale-to-Zero MicroVM**: Buffers incoming HTTP requests, signals Orchestrator via IPC to resume the MicroVM in <1.20 ms, and delivers the request with zero dropped packets.

### Running the Edge Ingress Proxy
```bash
# Automatically loads config/routes.local.json or config/routes.json if present
mos edge --port 8180

# Or specify a custom configuration file path
mos edge --port 8180 --config /etc/mos/production-routes.json
```

---

## 2. Dynamic MicroVM Ingress (`mos deploy`)

When developers deploy code, manual routing configuration is completely unnecessary:

1. **Automatic Registration**:
   ```bash
   mos deploy ./my-nextjs-app
   ```
   * Upon build completion, `mos-builder` dynamically registers `my-nextjs-app.mos.local -> 172.16.0.2:3000` into `mos-edge`'s in-memory routing table.
2. **Scale-to-Zero Idle Transition**:
   * After 300 seconds of inactivity, `mos-orchestrator` takes a memory snapshot, shuts down the process, and marks the route as `is_suspended = true`.
3. **Autonomous Wake-on-HTTP**:
   * When a client connects to `my-nextjs-app.mos.local`, `mos-edge` buffers the request, wakes the MicroVM in 1.20 ms, resets `is_suspended = false`, and forwards the payload seamlessly.

---

## 🛡️ Advanced Capabilities

### 1. 3-Stage Weighted Canary Deployments
Manage gradual traffic cutovers with automated error detection and instant HMAC rollbacks:
```
10% (Canary) -> 50% (Evaluation) -> 100% (Full Promotion)
```

### 2. Automated TLS & ACME
* Dynamic wildcard certificates and individual domain issuance via Let's Encrypt HTTP-01 challenges and SNI routing.

### 3. eBPF / XDP DDoS Defense
* Kernel-level packet filter drops malicious flooding exceeding rate limits before reaching proxy worker threads.

### 4. W3C Distributed Tracing (`traceparent`)
* Propagates W3C trace context and measures granular microsecond-level latency across `ingress`, `routing`, `wake`, and `guest_exec` execution phases.
