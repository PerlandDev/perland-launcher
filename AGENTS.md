# Perland Launcher: agent guide

- Design: `docs/superpowers/specs/2026-10-09-launcher-core-design.md`. Plans: `docs/superpowers/plans/`.
- `crates/launcher-core` holds all logic and must never depend on Tauri/UI crates. `crates/perland-cli` is a dev harness.
- TDD: write the failing test first. Pure logic gets `#[cfg(test)]` unit tests; HTTP behaviour gets `tests/*.rs` with wiremock.
- Never run fuzzing, stress or load tests locally. They run in CI only. Keep local test inputs tiny.
- Errors shown to players are `LauncherError` (category + plain message + optional fix + detail). Never put tokens in logs.
- UI styling and design-system work goes through the `/impeccable` skill.
- License GPL-3.0-only. Never copy Modrinth branding. Ported code keeps an attribution header and gets a `NOTICE` entry.
- Before committing: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.
