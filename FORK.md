# Personal Fork Maintenance

This is KazeMae's independently maintained fork of
[xai-org/grok-build](https://github.com/xai-org/grok-build), not an official release.
The upstream license and notices remain unchanged.

## Repositories and Branches

- `origin`: `git@github.com:KazeMae/grok-build.git`, the default push destination.
- `upstream`: `git@github.com:xai-org/grok-build.git`, the official source.
- `main`: the maintained personal version, tracking `origin/main`.
- `fix/responses-keepalive`: the initial isolated compatibility patch.

For a fresh clone:

```sh
git clone git@github.com:KazeMae/grok-build.git
cd grok-build
git remote add upstream git@github.com:xai-org/grok-build.git
git config remote.pushDefault origin
```

Keep personal changes in separate commits. Merge official updates into a review
branch before updating `main`; do not force-push shared branch history. Credentials,
local Grok configuration, sessions, and agent state such as `.omc/` stay outside
commits. Stage named files instead of the whole worktree.

## Initial Version

Version: `1.0.16+keepalive.1` (tag: `v1.0.16+keepalive.1`).

Upstream base: `72a61251fcff` (upstream package version `1.0.16`).
`SOURCE_REV` continues to identify the upstream monorepo snapshot, not fork commits.

Patch `7d71ca4b` changes the Responses streaming client to ignore gateway heartbeats
identified by the SSE event name `keepalive` or the top-level JSON `type` value
`keepalive`. Other unknown events still fail parsing. Tool calls, argument deltas,
server errors, stream termination, and idle timeouts retain their existing behavior.
No gateway changes or switch to Chat Completions are required.

Validation of the initial patch on macOS Apple Silicon:

- Five new HTTP-stream tests fail before the fix and pass afterward.
- All 239 sampler library tests pass when run serially.
- Formatting, strict Clippy, and the optimized application build pass.
- A live GPT Responses request completes a `read_file` call and a subsequent model
  reply, with two model requests and the expected file contents returned.

One existing tracing test, `first_use_span_records_freshness_once_per_origin`,
failed in a parallel suite run and passed in the serial suite. Use the serial
command below for this baseline; the tracing test itself is unchanged.

## Verify and Build

Install the toolchain in `rust-toolchain.toml` and DotSlash as described in
[README.md](README.md#building-from-source). The initial version uses Rust 1.94.0.

```sh
cargo test --locked -p xai-grok-sampler --lib -- --test-threads=1
cargo clippy --locked -p xai-grok-sampler --lib --tests --no-deps -- -D warnings
rustfmt --edition 2024 --check \
  crates/codegen/xai-grok-sampler/src/client.rs \
  crates/codegen/xai-grok-sampler/src/client_keepalive_tests.rs

GROK_VERSION=1.0.16+keepalive.1 cargo build --locked -p xai-grok-pager-bin --release
./target/release/xai-grok-pager --version
```

The artifact is `target/release/xai-grok-pager`. Install it as a new versioned
binary under `~/.grok/bin/`, back up the existing binary and configuration, and
atomically replace the `grok` symlink only after verification. Retain the old
binary for rollback. Running Grok sessions keep their old executable until restart.

The official updater is **not** redirected to this fork. To retain a custom build,
set this in the existing `[cli]` section of `~/.grok/config.toml`:

```toml
[cli]
auto_update = false
```

`grok update` and the official install script can replace the patch with an
official binary. Updating this fork currently means building and installing it
manually; creating a GitHub source tag does not publish a binary release.

## Synchronize Official Changes

Start with committed changes and choose a new branch name for each synchronization:

```sh
git fetch upstream --prune
git switch main
git pull --ff-only origin main
git switch -c sync/upstream-YYYYMMDD
git merge upstream/main
```

Resolve conflicts without dropping personal patches. Inspect the upstream diff,
run the verification commands, and exercise a complete Responses tool-call round
trip with the newly built binary. If upstream has fixed heartbeats, remove the
redundant local implementation while retaining regression coverage.

After verification:

```sh
git switch main
git merge --ff-only sync/upstream-YYYYMMDD
git push origin main
```

Use a new `GROK_VERSION` value and annotated tag for each published personal
version, preserving the upstream numeric version (for example,
`1.0.16+keepalive.2`). Update this document's patch and validation records for the
new version. Do not change the generated root `Cargo.toml` just to stamp a local
binary version.
