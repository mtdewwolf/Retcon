# Contributing to Retcon

Thanks for your interest! Retcon is early — expect churn while the foundation settles.

## Ground rules

- Read the product definition in [docs/product/](docs/product/README.md) first. The MVP
  scope is frozen; PRs adding out-of-scope features will be declined regardless of quality.
- The architectural non-negotiables in
  [docs/product/principles.md](docs/product/principles.md) are binding: no Electron, no
  WebView2 for the primary UI, no custom rendering engine.
- Significant design decisions get an ADR in [docs/adr/](docs/adr/) before or alongside the
  code.

## Development setup

1. Install the pinned Rust toolchain (`rustup` picks it up from `rust-toolchain.toml`).
2. Install Flutter (stable channel) and enable Windows desktop: `flutter config --enable-windows-desktop`.
3. Install [Bun](https://bun.sh) ≥ 1.1.
4. Build everything:
   - `cargo build --workspace`
   - `cd apps/desktop && flutter pub get && flutter analyze`
   - `cd apps/browser-service && bun install && bun test`

## Before you open a PR

- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` pass.
- `flutter analyze` and `flutter test` pass in `apps/desktop`.
- `bun test` passes in `apps/browser-service`.
- Commits are focused; the PR description explains *why*, not just *what*.

## Code style

- Rust: `rustfmt` defaults + workspace Clippy config. Errors are structured (`thiserror`);
  logging goes through `tracing`.
- Dart: `flutter_lints` as configured in `analysis_options.yaml`.
- TypeScript: strict mode; formatting/linting via the configs in `apps/browser-service`.

## Conduct

Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).
