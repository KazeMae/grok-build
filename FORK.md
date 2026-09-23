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
  [GrokZen](https://github.com/Catapult291/GrokZen). The overlay command is
  `grokx`; official grok-build keeps `grok`. Official auto-update and GrokZen
  installers are not part of this overlay.

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

Version: `1.0.41+kazemae.9` (not tagged yet). Review branch
`merge/upstream-1.0.41`; not on `main` until CI.

Upstream base is `07e35a3d` (upstream package version `1.0.41`,
`SOURCE_REV` `84745de98b3d`), merged in `27da0dba`. That single monorepo sync
jumps 1.0.38–1.0.41 (419 files, +25k/−6k). Git reported 27 content conflicts,
again the GrokZen pager overlay against upstream refactors: locale threading in
`worktree_cmd` / `disk_usage_cmd`, the settings render split
(`wrap_expanded_description` now takes text, `wrapped_description_height`
returns wrapped lines), the session-summary and subagent label helpers, and the
new `save_success_toast_with_locale` call site in `setters.rs`. The privacy
Mixpanel no-op also conflicts with upstream's `engage` / `base_url` / `post`
refactor; the overlay keeps the hard no-op and drops the transmission
internals. Overlay resolution keeps upstream structure and re-threads locale /
CJK / privacy on top, and adds the new-version community changelog
(`1.0.41.zh-CN.md` / `.json`) that `extract_builtin_files` includes.

Two upstream removals are followed rather than re-added: the *disabled* inline
edit-and-resubmit feature (`dispatch_inline_edit_submit`, `pending_inline_resubmit`)
and the workflow agent-count columns. `subagent_type` no longer feeds the
subagent label; upstream keys it on persona → role → tag → `subagent`.

The overlay command is `grokx`. Cargo still builds `xai-grok-pager`; install it
as `$GROK_HOME/bin/grokx` and leave `$GROK_HOME/bin/grok` for official
grok-build. Official auto-update still targets `bin/grok`. GrokZen installers
are not included.

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
  `"claude"`. Upstream still keys compact on `model_family`, which custom uniapi
  entries omit, so the Claude-name trigger remains required. It is stored on
  `SessionModelSwitch.is_family_switch`.
- `81904d59` adds a native Gemini `generateContent` REST/SSE backend
  (`api_backend = "gemini"`), distinct from OpenAI-compatible Chat Completions.
  `ApiBackend::default_max_request_bytes` (new upstream) now also covers the
  `Gemini` arm.

Upstream still implements neither heartbeat ignore nor reasoning omission, so those
two patches remain required. Telemetry stays compile-time locked. The Gemini backend
is also personal; upstream still has Chat Completions, Responses, and Messages only.

Validation of `1.0.41+kazemae.9` on macOS Apple Silicon (Rust 1.94.0):

- `cargo check --locked -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel`
  and `cargo clippy --locked -p xai-grok-pager -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel -- -D warnings`
  passed after overlay fixups.
- `cargo test --locked -p xai-grok-sampler -p xai-grok-sampling-types --lib -- --test-threads=1`
  passed (254 + 300 tests), covering the keepalive and reasoning-portability patches.
- `cargo fmt --all -- --check` is clean.
- `cargo test -p xai-grok-pager --lib` still fails to compile on the same 44
  upstream `#[cfg(test)]` call sites as `main` (verified identical); `CI` runs
  `clippy`/`build` without `--all-targets`, so this is unchanged and pre-existing.
- GitHub Actions `fmt / clippy / build` on the merge PR is the remaining gate.

## Previous Versions

### 1.0.38+kazemae.8

Not tagged. Upstream base was `4247f661` (package version `1.0.38`,
`SOURCE_REV` `9bb727ccdff0`), merged in `288d6842`. That snapshot jumped
1.0.36–1.0.38 in one monorepo sync (403 files) with 11 content conflicts, again
the GrokZen pager overlay against dashboard preview, prompt wrap, headless
locale, background-task verbs, and related structure changes. Overlay
resolution kept upstream structure and re-threaded locale / CJK / privacy on
top, including `TitleState.locale`, `Option<Duration>` session-event copy, and
dashboard peek locale. Validation: local `check` and strict `clippy` on the
pager/shell/mixpanel set passed; `cargo clippy --workspace --all-targets` still
failed on upstream test/bench targets.

### 1.0.35+kazemae.7

Not tagged. Upstream base was `a28ee2b2` (package version `1.0.35`,
`SOURCE_REV` `e8563f8f1822`), merged in `ff877f4e`. That snapshot jumped
1.0.33–1.0.35 in one monorepo sync (356 files). Git reported 21 content
conflicts, again the GrokZen pager overlay against new strings, `/memory`
rewrite, session-create timeout naming, and prompt `tool_calling` removal.

Validation of `1.0.35+kazemae.7` on macOS Apple Silicon (Rust 1.94.0):

- Local `cargo check --locked -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel`
  and `cargo clippy --locked -p xai-grok-pager -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel -- -D warnings`
  passed after overlay fixups.
- GitHub Actions `fmt / clippy / build` on the merge PR is the remaining gate.
- `cargo clippy --workspace --all-targets -- -D warnings` still fails on
  upstream test and bench targets; treat only findings inside overlay files as
  personal regressions.

### 1.0.32+kazemae.6

Not tagged. Upstream base was `48271133` (package version `1.0.32`,
`SOURCE_REV` `be7ce6e8cffe`), merged in `57e01196`. That snapshot jumped
1.0.25–1.0.32 in one monorepo sync (1650 files). Git reported 78 content
conflicts, mostly the GrokZen pager overlay against the 1.0.30 session-header
and dashboard rewrite.

Validation of `1.0.32+kazemae.6` on macOS Apple Silicon (Rust 1.94.0):

- Local `cargo check -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel`
  and `cargo clippy -p xai-grok-pager -p xai-grok-pager-bin -p xai-grok-shell -p xai-mixpanel -- -D warnings`
  passed after overlay fixups.
- GitHub Actions `fmt / clippy / build` on the merge PR is the remaining gate.
- `cargo clippy --workspace --all-targets -- -D warnings` still fails on
  upstream test and bench targets; treat only findings inside overlay files as
  personal regressions.


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
[README.md](README.md#building-from-source). Version `1.0.41+kazemae.9` uses
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

GROK_VERSION=1.0.41+kazemae.9 cargo build --locked -p xai-grok-pager-bin --release
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
`1.0.41+kazemae.9`). Update this document's patch and validation records for the
new version. Do not change the generated root `Cargo.toml` just to stamp a local
binary version.
