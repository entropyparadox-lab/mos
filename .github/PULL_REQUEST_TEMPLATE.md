## Description
Please include a summary of the change, the problem it solves, and relevant motivation/context.

Fixes #(issue)

## Type of Change
- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Performance improvement
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update
- [ ] Refactoring / Test addition

## Crates Affected
- [ ] `crates/mos-core`
- [ ] `crates/mos-orchestrator`
- [ ] `crates/mos-edge`
- [ ] `crates/mos-builder`
- [ ] `crates/mos-init`
- [ ] `crates/mos-cluster`
- [ ] `crates/mos-cli`
- [ ] `docs/` or `examples/`

## Verification & Testing
Describe how you tested these changes:
1. `cargo test --workspace` result
2. `cargo fmt --check` and `cargo clippy --workspace --all-targets` status
3. Any hardware KVM / MicroVM manual verification performed

## Checklist
- [ ] My code follows the code style of this project (`cargo fmt`).
- [ ] I have performed a self-review of my own code.
- [ ] I have commented my code, particularly in hard-to-understand areas.
- [ ] I have added tests that prove my fix is effective or that my feature works.
- [ ] New and existing tests pass locally with my changes.
- [ ] My changes generate no new compiler or Clippy warnings.
- [ ] I have updated the documentation where appropriate.
