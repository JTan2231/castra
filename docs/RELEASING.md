# Releasing Castra

This document tracks the steps for cutting an official Castra release. It is deliberately lightweight so that contributors can follow the same checklist when publishing a new version.

## Prerequisites

- Rust toolchain **1.77** (the project’s MSRV) and the latest stable channel installed via `rustup`.
- A clean working tree with no uncommitted changes.
- Access to the Castra git repository with permission to push tags.

## Release Checklist

1. **Update metadata**
   - Bump the version in `Cargo.toml` and regenerate `Cargo.lock` (`cargo update -p castra`).
   - Ensure any documentation updates for the release are committed (README, docs, etc.).
2. **Verify the build on MSRV**
   - `rustup run 1.77.0 cargo fmt`
   - `rustup run 1.77.0 cargo test`
   - Optionally run `cargo clippy --all-targets --all-features` and address warnings.
3. **Smoke-test the CLI**
   - `cargo run -- --version` should report the bumped semver and the current short git SHA.
   - Run representative commands against a sample project (`up`, `status`, `down`) to spot regressions.
4. **Tag the release**
   - `git tag -a vX.Y.Z -m "Castra vX.Y.Z"`
   - `git push origin vX.Y.Z`
5. **Publish crates.io artifacts**
   - `cargo publish`
   - Wait for publication to propagate, then verify `castra --version` from the registry build (no git metadata) prints the plain semver.
6. **Share release notes**
   - Draft release notes summarizing major changes and TODO updates.
   - Announce in the appropriate channels once the crate and binaries are available.

## Post-release

- Create or update the roadmap/TODO files under `.vizier` to reflect the new baseline.
- Reset any temporary branches or local state used for verification.
