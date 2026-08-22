# Security Policy

The MOS team takes the security of our platform, guest isolation boundaries, and user workloads seriously. We appreciate your efforts to responsibly disclose security vulnerabilities.

---

## Supported Versions

We provide security updates and patches for the following versions of MOS:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

---

## Reporting a Vulnerability

If you believe you have discovered a security vulnerability in MOS (including KVM isolation escapes, vsock IPC vulnerabilities, RBAC bypasses, or proxy denial of service), please report it to us privately.

**Please do NOT disclose security vulnerabilities publicly through GitHub Issues, Discussions, or Pull Requests.**

### How to Report

1. Send an email to **`security@entropyparadox.com`** with the subject `[MOS Security Disclosure] <Short Description>`.
2. Include the following details in your report:
   - Type of vulnerability (e.g., Guest Escape, Memory Corruption, RBAC Bypass, Buffer Overflow).
   - Component affected (`mos-orchestrator`, `mos-edge`, `mos-core`, `mos-init`, `mos-builder`, `mos-cluster`).
   - Detailed step-by-step reproduction steps or Proof of Concept (PoC) code.
   - Impact of the vulnerability and potential attack vectors.
   - Any proposed remediation or patch if available.

### Response Process

* **Acknowledgement**: We will acknowledge receipt of your vulnerability report within 48 hours.
* **Assessment**: Our core maintainers will verify the vulnerability and evaluate its severity.
* **Remediation**: We will prepare a patch, test it across all supported environments, and coordinate a public release timeline with you.
* **Credit**: We will credit your contribution in the security advisory and release notes (unless you prefer to remain anonymous).

---

## Scope & Threat Model

The primary security boundaries of MOS include:
1. **MicroVM Boundary**: The guest operating system and user application running inside Firecracker must never gain unprivileged access to the host or other tenant MicroVMs.
2. **Control Plane & Edge Authentication**: Ed25519 RBAC tokens and tenant resource limits must be strictly enforced. Cross-tenant access must be rejected.
3. **eBPF/XDP Rate Limiter**: Malicious packet flooding targeting the ingress edge must be mitigated at kernel level without crashing the proxy.

Security reports concerning out-of-scope third-party infrastructure (e.g., vulnerabilities in upstream unpatched Linux kernels without MOS-specific triggers) may be redirected to upstream maintainers.
