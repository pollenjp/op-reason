# No console window when no terminal starts op-reason.exe — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On Windows 11 24H2+ and Windows Server 2025, the SSH relay that Task Scheduler starts (logon, Apply, the
icon's restart) shows no console window, and nothing else changes for terminals and WSL.

**Architecture:** `op-reason.exe`'s embedded manifest gets `consoleAllocationPolicy` `detached`, so Windows makes no
console for it unless one is inherited. Two adjustments keep every other start as it is: `op.exe` gets
`CREATE_NO_WINDOW` when op-reason.exe has no console, and a bare `op-reason.exe` with no console and no standard output
counts as a double click. The relay names the console it had in its `start` record, which the Windows tests check.

**Tech Stack:** Rust 1.99.1 (`crates/op-reason-win`, its own Cargo workspace), the `windows` crate 0.62.2,
`embed-manifest` 1.5.1, GitHub Actions (`windows-2025`, `windows-2022`).

**Spec:** `docs/superpowers/specs/2026-11-24-relay-no-console-window-design.md`

## Global Constraints

- The element, exactly: `<consoleAllocationPolicy xmlns="http://schemas.microsoft.com/SMI/2024/WindowsSettings">detached</consoleAllocationPolicy>`, inside `<asmv3:windowsSettings>` of the manifest `build.rs` already builds (name `op-reason`, the version from `Cargo.toml`, Per-Monitor V2).
- No new dependency, no second exe, no change to op-reason's task, `relay.json`, the zip's entries or the version (0.6.0 stays).
- The `start` record's `detail` values, exactly: `console: none`, `console: own`, `console: shared`.
- Everything a terminal sees stays: output, exit codes, waiting, Ctrl+C (`2026-11-15-windows-exe-design.md` §8.2: `op.exe` shares a console op-reason.exe has).
- Public repository: no ticket IDs, user names, host names or local paths in files, commits or the pull request; other repositories only as plain text (no `owner/repo#n`, no PR or commit URLs in commit messages or the title).
- Lints as CI runs them: `cargo fmt --check -p op-reason-win -p fake-op`, `cargo clippy --locked --workspace --all-targets -- -D warnings` (Linux and Windows), the main workspace's `cargo fmt`/`clippy`/`test` for the core.
- READMEs in English and Japanese change together; `CHANGELOG.md` under Unreleased.

## Review Focus

- A call from WSL without a terminal (WSL starts `op-reason.exe` with `CREATE_NO_WINDOW` and pipes), and one from a program without a console that passes pipes: output, exit code and no console window for `op.exe` — Task 3's test `a_call_without_a_console_shows_no_console_window_for_op`.
- A call from WSL with a terminal (a pseudoconsole): the output still reaches the terminal — Task 3's test `a_pseudoconsole_is_the_console_of_a_call`.
- A bare `op-reason.exe` whose output a program reads (a pipe) prints the usage, never opens the window — the existing `no_arguments_prints_usage_and_records_nothing` (exe.rs) keeps it.
- Older Windows do not refuse to start an exe whose manifest has an element they do not know — Task 4's job on `windows-2022`.
- A relay started from a terminal stays attached (Ctrl+C, the exit record) — Task 1 keeps `attached` for `console: shared`; the existing relay tests that start relays from the harness keep it.

---

### Task 1: The relay names its console at start, and the tests that see the flash (red on the runner)

**Files:**
- Create: `crates/op-reason-win/tests/common/console_watch.rs`
- Modify: `crates/op-reason-win/tests/settings.rs` (move `each_console`, `visible_consoles`, `ConsoleWatch` out; add the task test; extend `the_icon_restarts_the_relay_of_op_reasons_task`)
- Modify: `crates/op-reason-win/src/win/relay/mod.rs` (`detach_console`, the `start` record)
- Modify: `crates/op-reason-core/src/ssh/record.rs` (the doc of `Relay::detail`)

**Interfaces:**
- Produces: `tests/common/console_watch.rs` with `pub struct ConsoleWatch`, `pub fn start(needle: &Path) -> ConsoleWatch`, `pub fn stop(self) -> Vec<String>` (each `"<class>: <title>"` seen), used with `#[path = "common/console_watch.rs"] mod console_watch;` and `#![allow(dead_code)]` in the module.
- Produces: in `win/relay/mod.rs`, `enum Console { None, Own, Shared }` with `fn detail(self) -> &'static str` and `fn detach_console() -> Console` (`GetConsoleProcessList`: 0 → `None`, 1 → `FreeConsole` and `Own`, more → `Shared`); `attached` is `console == Console::Shared`; the `start` record is written with `relay_record_with("start", parent, Some(console.detail().to_owned()))`.

- [ ] **Step 1: Move the watcher into the shared test module** (no behavior change) and use it from `settings.rs`.
- [ ] **Step 2: Write the failing test** `a_relay_of_op_reasons_task_starts_without_a_console` in `settings.rs` (CI only, `serial()`, a test task through `task_settings`):

```rust
let watch = ConsoleWatch::start(&exe());
let out = s(&["set", "relay.autostart", "on"]);            // the task's run, as at logon
assert_eq!(out.status.code(), Some(0), "{}", text(&out));
assert!(wait_until(Duration::from_secs(10), || relay_json_pid().is_some()));
let first = relay_json_pid().unwrap();
assert_eq!(start_detail(first).as_deref(), Some("console: none"));
let out = s(&["relay", "restart"]);                         // what Apply runs
assert_eq!(out.status.code(), Some(0), "{}", text(&out));
assert!(wait_until(Duration::from_secs(10), || relay_json_pid().is_some_and(|p| p != first)));
assert_eq!(start_detail(relay_json_pid().unwrap()).as_deref(), Some("console: none"));
let shown = watch.stop();
assert!(shown.is_empty(), "no console window for the task's relay: {shown:?}");
```

  `fn start_detail(pid: u64) -> Option<String>`: the `detail` of the last `ssh-relay` `start` record with that `pid` in `%LOCALAPPDATA%\op-reason\ssh-log.jsonl`. `relay_json_pid` moves above its first use.
- [ ] **Step 3: Extend** `the_icon_restarts_the_relay_of_op_reasons_task`: a `ConsoleWatch::start(&exe())` from before `set relay.autostart on` to after the new relay answers, `shown.is_empty()`, and `start_detail(<new pid>) == Some("console: none")`.
- [ ] **Step 4: Implement** `Console`, `detach_console() -> Console` and the `start` detail in `win/relay/mod.rs`; update `Relay::detail`'s doc ("the step of a window that does not appear; at start, the console the relay had").
- [ ] **Step 5: Check locally**: `cd crates/op-reason-win && cargo fmt -p op-reason-win -p fake-op && cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings && cargo clippy --locked --workspace --all-targets -- -D warnings && cargo test --locked --workspace` (Linux: the portable parts). Expected: clean.
- [ ] **Step 6: Commit** `test(win): a relay of op-reason's task shows no console window` and push; open the pull request as a draft. Expected on the runner: the two task tests FAIL (`console: own`, and the watcher sees `ConsoleWindowClass` or `CASCADIA_HOSTING_WINDOW_CLASS` titled with the exe's path) — the bug, seen.

### Task 2: The manifest's console allocation policy

**Files:**
- Modify: `crates/op-reason-win/build.rs`
- Test: `crates/op-reason-win/tests/exe.rs` (`the_exe_asks_for_no_console_of_its_own`)

**Interfaces:**
- Produces: `fn with_console_policy(xml: &str) -> String` in `build.rs` (panics unless `</asmv3:windowsSettings>` occurs exactly once); the manifest file `$OUT_DIR/op-reason.exe.manifest`, embedded with `embed_manifest::embed_manifest_file`.

- [ ] **Step 1: Write the failing test** `the_exe_asks_for_no_console_of_its_own` (exe.rs): the bytes of `exe()` contain `consoleAllocationPolicy xmlns="http://schemas.microsoft.com/SMI/2024/WindowsSettings">detached</consoleAllocationPolicy>` and still `PerMonitorV2` (the manifest is UTF-8 text in the resources).
- [ ] **Step 2: Implement** `with_console_policy` and the file embedding in `build.rs`; its module doc names the policy and the spec.
- [ ] **Step 3: Check locally** as in Task 1 Step 5 (the Windows-target clippy runs the build script).
- [ ] **Step 4: Commit** `fix(win): op-reason.exe asks Windows for no console it does not inherit` and push. Expected on the runner: Task 1's tests and `the_exe_asks_for_no_console_of_its_own` PASS; `a_double_click_opens_the_window_instead_of_the_usage` FAILS (Explorer's start now has no console). Seen: so, and Task 3's tests passed too — `CREATE_NO_WINDOW` keeps its console (spec §3.1) — so Task 3's first test also starts the call with `DETACHED_PROCESS`.

### Task 3: Starts without a console stay as they were

**Files:**
- Modify: `crates/op-reason-win/src/win/console.rs` (`none`, `started_alone`, `has_handle`; `of_its_own` goes)
- Modify: `crates/op-reason-win/src/win/app.rs:144` (the bare start)
- Modify: `crates/op-reason-win/src/win/opexe.rs` (`start`: the flags)
- Modify: `crates/op-reason-win/src/win/settings/window.rs` (the doc of `start_detached`)
- Test: `crates/op-reason-win/tests/exe.rs`

**Interfaces:**
- Produces: `pub fn none() -> bool` (`GetConsoleProcessList` returns 0) and `pub fn started_alone() -> bool` (0 processes and no standard output handle, or 1 process and standard output a console) in `win/console.rs`.

- [ ] **Step 1: Write the failing tests** in `exe.rs` (with `mod console_watch` as in Task 1), pushed with Task 2:
  - `a_call_without_a_console_shows_no_console_window_for_op`: `ConsoleWatch::start(&fake_op())`; the entry face `vault --help` and the runner face (`--run`, a `help` payload with `--version`), each spawned with `CREATE_NO_WINDOW` (as WSL) and with `DETACHED_PROCESS` (no console, as the policy leaves a start without flags), piped stdin/stdout/stderr, `FAKE_SLEEP_MS=1500`, stdin `b"through the pipe"`; each exits 0 and its stdout is `through the pipe`; `watch.stop()` is empty.
  - `a_pseudoconsole_is_the_console_of_a_call`: `op-reason.exe --version` under `CreatePseudoConsole` (STARTUPINFOEX with `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE`, `STARTF_USESTDHANDLES` with null handles, no inherited handles, as WSL's interop does): exit 0, and the pseudoconsole's output, read on a thread until `ClosePseudoConsole`, contains `op-reason 0.6.0` (`env!("CARGO_PKG_VERSION")`).
- [ ] **Step 2: Implement** `none`, `started_alone` (replacing `of_its_own` in `app.rs`), and in `opexe::start` the flags `CREATE_SUSPENDED | CREATE_NO_WINDOW` when `console::none()`, else `CREATE_SUSPENDED`; docs say why (spec §5.2, §5.3).
- [ ] **Step 3: Check locally** as in Task 1 Step 5.
- [ ] **Step 4: Commit** `fix(win): a start without a console keeps op.exe's console hidden and opens the window on a double click` and push. Expected on the runner: every test PASSES.

### Task 4: Older Windows still start the exe

**Files:**
- Modify: `.github/workflows/windows-exe.yml` (a job `older-windows`)

- [ ] **Step 1: Add the job**: `needs: windows`, `runs-on: windows-2022`, `timeout-minutes: 10`; download the artifact `op-reason-windows` (the pinned `actions/download-artifact` of the workflow), `Expand-Archive`, then in pwsh: `& $exe --version` prints `op-reason <version>` with exit 0, and `& $exe --ssh-agent --status` exits 1 with one line. The job's comment says why (build 20348 does not know the policy).
- [ ] **Step 2: Commit** `ci: op-reason.exe starts on Windows Server 2022` and push. Expected: the job PASSES.

### Task 5: Documents

**Files:**
- Modify: `README.md`, `README.ja.md` ("Starting it", "The window, the Start menu and the icon", the notes of "On Windows")
- Modify: `CHANGELOG.md` (Unreleased)
- Modify: `docs/manual-verification.md`, `docs/manual-verification.ja.md` (N1–N11 after the settings' checks; G11 and G15's console)
- Modify: `docs/architecture/README.md` (the relay's threads and start; the settings window's process; the build's manifest)

- [ ] **Step 1: Write** the texts from the spec (§1, §5, §8, §9): on Windows 11 24H2 and later the relay of a task, a double click and the Start menu show no console window; older Windows show one for a moment; a start with arguments and without a terminal gets no console there.
- [ ] **Step 2: Check**: `grep -n "console" README.md README.ja.md` reads the same in both; markdown tables keep their columns.
- [ ] **Step 3: Commit** `docs: no console window when no terminal starts op-reason.exe` and push. Expected: CI green; mark the pull request ready.
