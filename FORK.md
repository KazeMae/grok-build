# Personal Fork Maintenance

This is KazeMae's independently maintained fork of
[xai-org/grok-build](https://github.com/xai-org/grok-build), not an official release.
The upstream license and notices remain unchanged.

## Repositories and Branches

- `origin`: `git@github.com:KazeMae/grok-build.git`, the default push destination.
- `upstream`: `git@github.com:xai-org/grok-build.git`, the official source.
- `main`: the maintained personal version, tracking `origin/main`.
- `fix/responses-keepalive`: the initial isolated compatibility patch.
- `fix/outbound-foreign-thinking`: the cross-backend reasoning-omission patch.
- `merge/upstream-<version>`: the review branch for one official synchronization.

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

## Current Version

Version: `1.0.24+kazemae.2` (tag: `v1.0.24+kazemae.2`).

Upstream base: `75810042ca27` (upstream package version `1.0.24`, `SOURCE_REV`
`eb4a894da8fb`), merged in `29572058` without conflicts. `SOURCE_REV` continues to
identify the upstream monorepo snapshot, not fork commits. The `+keepalive.N` suffix
of the initial version is retired; personal versions now carry `+kazemae.N` because
the fork holds more than the heartbeat patch.

Personal patches on top of that base:

- `7d71ca4b` makes the Responses streaming client ignore gateway heartbeats
  identified by the SSE event name `keepalive` or the top-level JSON `type` value
  `keepalive`. Other unknown events still fail parsing. Tool calls, argument deltas,
  server errors, stream termination, and idle timeouts retain their existing
  behavior. No gateway changes or switch to Chat Completions are required.
- `4b0c930d` omits reasoning blobs the destination protocol cannot verify from
  outbound Messages and Responses requests: OpenAI `gAAAAA` and xAI `tco_`
  signatures on Messages, Anthropic `CA` and empty-id items on Responses. A
  mid-session backend switch then keeps the transcript instead of needing a lossy
  compact.

Upstream 1.0.24 implements neither behavior, so both patches are still required.
Upstream changed six of the twelve files these patches touch, all outside the
patched hunks: comment rewrites in the conversation types and their tests, and new
mTLS, conversation-group header, and span-timing code in the sampler client.

Validation of `1.0.24+kazemae.2` on macOS Apple Silicon (Rust 1.94.0):

- 241 sampler and 286 sampling-types library tests pass when run serially,
  including the five heartbeat tests and the reasoning-omission tests.
- The optimized `xai-grok-pager` build succeeds and reports the fork version.
- `cargo clippy --workspace --all-targets` fails on two upstream test targets that
  no personal patch touches: a disallowed `reqwest::Client::new` in
  `xai-tracing::http_client` tests and a `single_match` lint in the `xai-grok-tools`
  `read_file` tests. Both files are identical to their upstream 1.0.24 content, so
  the strict workspace-wide lint gate is an upstream condition, not a merge defect.
  The patched crates pass strict Clippy on their own.

## Initial Version

Version: `1.0.16+keepalive.1` (tag: `v1.0.16+keepalive.1`).

Upstream base: `72a61251fcff` (upstream package version `1.0.16`), carrying patch
`7d71ca4b` only.

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
[README.md](README.md#building-from-source). Version `1.0.24+kazemae.2` uses
Rust 1.94.0.

```sh
cargo test --locked -p xai-grok-sampler -p xai-grok-sampling-types --lib -- --test-threads=1
cargo clippy --locked -p xai-grok-sampler -p xai-grok-sampling-types \
  --lib --tests --no-deps -- -D warnings
rustfmt --edition 2024 --check \
  crates/codegen/xai-grok-sampler/src/client.rs \
  crates/codegen/xai-grok-sampler/src/client_keepalive_tests.rs \
  crates/codegen/xai-grok-sampling-types/src/conversation/reasoning_portability.rs

GROK_VERSION=1.0.24+kazemae.2 cargo build --locked -p xai-grok-pager-bin --release
./target/release/xai-grok-pager --version
```

`cargo clippy --locked --workspace --all-targets` additionally reports the two
upstream test-target lints recorded above. Treat new findings outside those two files
as regressions; the two known ones do not block a personal build.

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
git switch -c merge/upstream-<upstream-version>
git merge upstream/main
```

Resolve conflicts without dropping personal patches. Inspect the upstream diff,
run the verification commands, and exercise a complete Responses tool-call round
trip with the newly built binary. If upstream has fixed heartbeats or cross-backend
reasoning portability, remove the redundant local implementation while retaining
regression coverage.

After verification:

```sh
git switch main
git merge --ff-only merge/upstream-<upstream-version>
git push origin main
```

Use a new `GROK_VERSION` value and annotated tag for each published personal
version, preserving the upstream numeric version (for example,
`1.0.24+kazemae.3`). Update this document's patch and validation records for the
new version. Do not change the generated root `Cargo.toml` just to stamp a local
binary version.
