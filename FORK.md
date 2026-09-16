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
  When the upstream package version repeats, append the short `SOURCE_REV` of the
  new snapshot, as in `merge/upstream-1.0.24-c4ea71cf`.
- `merge/grokzen`: Simplified Chinese UI and compile-time privacy ported from
  [GrokZen](https://github.com/Catapult291/GrokZen). The executable remains `grok`.
  Official auto-update and GrokZen installers are not part of this overlay.

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

Version: `1.0.32+kazemae.6` (not tagged yet).

Upstream base is `48271133` (upstream package version `1.0.32`,
`SOURCE_REV` `be7ce6e8cffe`), merged in `57e01196`. That snapshot jumps
1.0.25–1.0.32 in one monorepo sync (1650 files). Git reported 78 content
conflicts, mostly the GrokZen pager overlay against the 1.0.30 session-header
and dashboard rewrite. Overlay resolution keeps upstream structure and
re-threads locale / CJK / privacy on top.

The executable name stays `grok` / `xai-grok-pager`. Official auto-update is
unchanged; GrokZen installers are not included.

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
- `1ab66565` ports GrokZen locale catalogs, default `zh-CN` UI, and the compile-time
  `privacy` feature (Mixpanel / product events / OTLP export hard-off).
- `c07120c2` adds the settings Appearance locale switch, `[session] length_salvage_budget`,
  and `/model` lossy compact when the target catalog id / slug / display name contains
  `"claude"`. Upstream 1.0.32 still keys compact on `model_family`, which custom uniapi
  entries omit, so the Claude-name trigger remains required. It is stored on
  `SessionModelSwitch.is_family_switch`.

Upstream still implements neither heartbeat ignore nor reasoning omission, so those
two patches remain required. Telemetry stays compile-time locked.

Validation of `1.0.32+kazemae.6` on macOS Apple Silicon (Rust 1.94.0):

- Local `cargo check -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel`
  and `cargo clippy -p xai-grok-pager -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel -- -D warnings`
  passed after overlay fixups.
- GitHub Actions `fmt / clippy / build` on the merge PR is the remaining gate.
- `cargo clippy --workspace --all-targets -- -D warnings` still fails on
  upstream test and bench targets; treat only findings inside overlay files as
  personal regressions.

## Previous Versions

### 1.0.24+kazemae.5

Tag `v1.0.24+kazemae.5` on `9c886c5a`. Upstream base was still `37949780c144`
(package version `1.0.24`, `SOURCE_REV` `c4ea71cfdbcd`). Added the GrokZen
Simplified Chinese UI and compile-time privacy overlay on top of the keepalive
and reasoning-omission patches. PR `#3` later added the locale switch, length
salvage budget, and Claude-name compact without a new tag.

### 1.0.24+kazemae.4

Tag `v1.0.24+kazemae.4` on `7953207e`, the GitHub Actions build/release pipeline.
No overlay changes.

### 1.0.24+kazemae.3

Upstream base: `37949780c144` (upstream package version `1.0.24`, `SOURCE_REV`
`c4ea71cfdbcd`), merged in `27b1e223` without conflicts. That snapshot changed 695
files and none of the twelve files the personal patches touch, so the merge needed no
fixups. The upstream package version did not move; only the monorepo snapshot did, so
the personal suffix carries the whole difference from the previous version.
`SOURCE_REV` continues to identify the upstream monorepo snapshot, not fork commits.
The `+keepalive.N` suffix of the initial version is retired; personal versions carry
`+kazemae.N` because the fork holds more than the heartbeat patch.

Validation of `1.0.24+kazemae.3` on macOS Apple Silicon (Rust 1.94.0):

- 242 sampler and 286 sampling-types library tests pass when run serially (upstream
  added one sampler test), including the five heartbeat tests and both
  reasoning-omission tests.
- Strict Clippy passes on the patched crates and on every workspace library and
  binary target.
- The optimized `xai-grok-pager` build succeeds and reports the fork version.
- A live Responses request (`gpt-6-astra`) and a live Messages request
  (`claude-fable-5-1`) each complete a `read_file` tool call and return the fixture
  token, run against the newly built binary on an isolated leader socket.

Adding `--all-targets` to the workspace Clippy run fails on 13 upstream test and
bench targets, none of them a file a personal patch touches. Some are lint
promotions on unchanged upstream code, and some are real upstream breakage: this
snapshot converted the PTY-harness scenario enums to `strum` derives while the
benches still call the removed `as_str`. Treat only findings inside patched files as
personal regressions. The earlier `1.0.24+kazemae.2` record listed just two findings
because that run stopped at the first failing target; `--keep-going` reveals the rest
on the same tree.

### 1.0.24+kazemae.2

Upstream base `75810042ca27` (package version `1.0.24`, `SOURCE_REV eb4a894da8fb`),
merged in `29572058`. First version to carry both personal patches, and the first
built after the upstream jump from `1.0.16` to `1.0.24`. Upstream had changed six of
the twelve patched files, all outside the patched hunks.

### 1.0.16+keepalive.1

The initial version (tag: `v1.0.16+keepalive.1`), upstream base `72a61251fcff`
(upstream package version `1.0.16`), carrying patch `7d71ca4b` only.

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
[README.md](README.md#building-from-source). Version `1.0.24+kazemae.5` uses
Rust 1.94.0.

```sh
cargo test --locked -p xai-grok-sampler -p xai-grok-sampling-types --lib -- --test-threads=1
cargo clippy --locked -p xai-grok-sampler -p xai-grok-sampling-types \
  --lib --tests --no-deps -- -D warnings
rustfmt --edition 2024 --check \
  crates/codegen/xai-grok-sampler/src/client.rs \
  crates/codegen/xai-grok-sampler/src/client_keepalive_tests.rs \
  crates/codegen/xai-grok-sampling-types/src/conversation/reasoning_portability.rs

cargo clippy --locked --workspace --no-deps --keep-going -- -D warnings

GROK_VERSION=1.0.24+kazemae.5 cargo build --locked -p xai-grok-pager-bin --release
./target/release/xai-grok-pager --version
```

Verify the built binary against both patched code paths before installing it, using
an isolated leader socket so the run cannot disturb a live session:

```sh
printf 'verify-token\n' > /tmp/grok-verify/answer.txt
cd /tmp/grok-verify
<build>/xai-grok-pager -p "Use the read_file tool on answer.txt, then reply with only \
  the exact token it contains." -m <responses-model> \
  --permission-mode dontAsk --leader-socket ~/.grok/leader-verify.sock
```

Repeat with a `messages` backend model. Adding `--all-targets` to the workspace
Clippy run also compiles upstream test and bench targets, which currently fail for
upstream reasons; run it with `--keep-going` when auditing, and treat only findings
inside patched files as personal regressions.

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
`1.0.24+kazemae.4`). Update this document's patch and validation records for the
new version. Do not change the generated root `Cargo.toml` just to stamp a local
binary version.
