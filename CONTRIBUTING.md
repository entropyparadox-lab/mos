# Contributing to MOS (MicroVM Operating Service)

Thank you for your interest in contributing to MOS! We are building a high-performance, Rust-native, scale-to-zero serverless platform on top of Linux KVM and Firecracker MicroVMs.

This guide outlines our development workflow, coding conventions, testing requirements, and submission process.

---

## Code of Conduct

All contributors and maintainers are expected to follow our [Code of Conduct](CODE_OF_CONDUCT.md). Please read it before participating in our discussions, issues, or pull requests.

---

## Getting Started

### Prerequisites

To develop, build, and test MOS locally, your environment must meet the following requirements:

1. **Linux Operating System (x86_64)**:
   - Ubuntu 22.04 LTS+, Debian 12+, Arch Linux, or Fedora.
   - MicroVM execution requires direct hardware virtualization access.
2. **KVM Enabled**:
   - Verify hardware virtualization is active: `kvm-ok` or `ls -l /dev/kvm`.
   - Ensure your user has read/write permissions to `/dev/kvm` (e.g., `sudo usermod -aG kvm $USER`).
3. **Cgroups v2**:
   - Ensure unified cgroups v2 hierarchy is mounted at `/sys/fs/cgroup`.
4. **Rust Toolchain**:
   - Rust 1.80+ (`rustup default stable`).
   - Cargo components: `rustfmt` and `clippy`.
5. **Runtime Dependencies & Binaries**:
   - Run `./scripts/setup-firecracker.sh` to download the appropriate `firecracker`, `jailer`, and `vmlinux` kernel binaries if you wish to run full hardware MicroVM tests.

### Fork & Clone

```bash
# Fork the repository on GitHub, then clone your fork
git clone https://github.com/<your-username>/mos.git
cd mos

# Build the entire workspace
cargo build --workspace
```

---

## Workspace Structure

MOS is organized as a Cargo workspace with 7 focused crates:

```
crates/
├── mos-core/          # Domain types, state machine, RBAC & billing models, ISTQB tests
├── mos-orchestrator/  # Firecracker process controller, snapshots, UFFD, Cgroups, GPU pool
├── mos-edge/          # Hyper-based ingress proxy, TCP buffering, TLS, canary routing
├── mos-builder/       # Nixpacks integration, rootfs ext4 generation, Litestream engine
├── mos-init/          # Static PID 1 guest supervisor binary (<820KB)
├── mos-cluster/       # SWIM Gossip node discovery & consistent hash ring
└── mos-cli/           # Operator CLI command line tool & Web Dashboard
```

---

## Development Workflow

### 1. Branch Naming

Create a feature branch from `main`:

```bash
git checkout -b feat/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

Branch naming conventions:
* `feat/<name>`: New features or capabilities
* `fix/<name>`: Bug fixes and edge case resolutions
* `perf/<name>`: Performance improvements and optimizations
* `docs/<name>`: Documentation updates
* `test/<name>`: Test harness additions or refactoring
* `refactor/<name>`: Code refactoring without behavior change

### 2. Commit Message Conventions

We follow the [Conventional Commits](https://www.conventionalcommits.org/) standard:

```
<type>(<scope>): <short summary>

[optional body]

[optional footer(s)]
```

**Types:**
* `feat`: A new feature
* `fix`: A bug fix
* `docs`: Documentation only changes
* `style`: Changes that do not affect the meaning of the code (formatting, white-space)
* `refactor`: A code change that neither fixes a bug nor adds a feature
* `perf`: A code change that improves performance
* `test`: Adding missing tests or correcting existing tests
* `chore`: Changes to the build process or auxiliary tools

**Examples:**
```
feat(edge): add support for W3C traceparent header propagation
fix(orchestrator): handle zombie process reaping during sudden guest crash
test(core): add boundary value analysis for tenant quota exhaustion
docs(readme): update latency benchmarks table with Ryzen 9700X results
```

---

## Code Quality & Testing

All pull requests must pass our automated CI verification before being merged.

### Running Tests

```bash
# Run all unit, integration, and adversarial tests
cargo test --workspace

# Run tests for a specific crate
cargo test -p mos-orchestrator
cargo test -p mos-edge
cargo test -p mos-core
```

### Code Formatting

We require all code to be formatted using `rustfmt`:

```bash
# Check formatting
cargo fmt --check

# Auto-format all code
cargo fmt
```

### Linting (Clippy)

All code must compile cleanly with zero Clippy warnings:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

---

## Pull Request Guidelines

1. **Keep Changes Focused**: Each PR should address a single concern, feature, or bug fix.
2. **Include Tests**: Add unit or integration tests that verify new behavior or reproduce and fix reported bugs.
3. **Update Documentation**: If your PR introduces new CLI flags, configuration options, or APIs, update the corresponding documentation in `docs/` and `README.md`.
4. **Self-Review**: Review your diff with `git diff` before submitting to ensure no debug statements, temporary files, or unintended formatting changes are included.
5. **PR Description**: Use our PR template to describe the problem, the solution, and the testing steps performed.

---

## Reporting Issues

* **Bug Reports**: Use the [Bug Report Template](.github/ISSUE_TEMPLATE/bug_report.md). Include your Linux kernel version, hardware specs, steps to reproduce, and relevant logs.
* **Feature Requests**: Use the [Feature Request Template](.github/ISSUE_TEMPLATE/feature_request.md). Explain the use case, why existing features are insufficient, and your proposed design.

---

## Security Disclosures

If you discover a security vulnerability, please do **NOT** open a public issue. Review our [Security Policy (SECURITY.md)](SECURITY.md) for private reporting instructions.

---

## License Notice

By contributing to MOS, you agree that your contributions will be licensed under its dual [MIT](LICENSE-MIT) and [Apache 2.0](LICENSE-APACHE) licenses.
