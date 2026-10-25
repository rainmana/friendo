# Tagged binary releases

Download native binaries from [GitHub Releases](https://github.com/rainmana/friendo/releases). Each release includes SHA256SUMS and four platform archives:

| Target | Build/verification host | Baseline |
| --- | --- | --- |
| x86_64-unknown-linux-gnu | Ubuntu 22.04 x86-64 | glibc 2.35 or newer |
| aarch64-apple-darwin | macOS 15 Apple Silicon | Deployment target macOS 13 |
| x86_64-apple-darwin | macOS 15 Intel | Deployment target macOS 13 |
| x86_64-pc-windows-msvc | Windows Server 2022 x86-64 | MSVC, static C runtime |

Older OS baselines are build targets, not evidence of execution on those older systems. macOS binaries are unsigned and not notarized; Windows binaries are unsigned. The current release does not include Linux arm64 or musl binaries.

Extract the archive, place `friendo` (or `friendo.exe`) on PATH, and run `friendo doctor` or `friendo demo`. Provider operation requires a local configuration with an explicitly available model. Copy the included `friendo.example.toml` when configuring one; it contains placeholders and credential references only. The example file is not loaded automatically. Each package includes LICENSE, README, and SOURCE.txt with the exact tag/commit and corresponding source URL. The QA fixture, workspace friendo.toml, credentials, generated runs, and index state are excluded.

On Linux use `sha256sum --check SHA256SUMS` after downloading all four archives. On macOS use `shasum -a 256 --check SHA256SUMS`. On Windows use `Get-FileHash <archive> -Algorithm SHA256` and compare its value with the matching SHA256SUMS entry. If downloading a single archive, compare only its matching checksum entry.

## Maintainer sequence

1. Update Cargo.toml/Cargo.lock together for the release version and add `docs/releases/v<VERSION>.md` release notes.
2. Run `bash scripts/check.sh` (including release publication guard tests), Python syntax validation for `scripts/release.py`, and actionlint. Build and exercise a local optimized package with the commands below.
3. Commit and push to main. Wait for native verification to pass on the exact commit.
4. Create and push an annotated `v<VERSION>` tag on that tested commit. The tag must exactly match Cargo's version and belong to main history.
5. Monitor `tagged release`. It reuses the successful native CI result, builds four optimized native application binaries, and smoke-tests each extracted package: version, offline NORMAL defaults, repaired synthetic demo, and byte-preserving replay.
6. Publication requires the complete four-platform archive set, matching source identity, and valid checksums. Assets are uploaded to a draft, then the workflow publishes it. Existing releases/assets are never overwritten automatically. If publication itself fails after staging, inspect the draft before completing it or retrying.

```console
cargo build --release --locked --bin friendo --target aarch64-apple-darwin
python3 scripts/release.py package --tag v0.1.0 --target aarch64-apple-darwin
```

Substitute the native target/version for another host. Output stays under ignored `target/release-assets/`. The package command refuses to overwrite existing assets. To retry a failed build workflow on an existing tag, rerun its failed jobs; manual dispatch is also supported when selecting the tag ref, after checking for any existing draft/published release.
