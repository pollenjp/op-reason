# op-reason: calls from WSL through op-reason.exe — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The WSL binary starts `op-reason.exe --run` for every call (the exe shows the window and starts `op.exe`),
falls back to `powershell.exe -File launcher.ps1` only when the exe left no status file, and no longer embeds the
launcher.

**Architecture:** The payload (v1) and the status file stay the contract. The exe gets a runner face that follows the
launcher's contract and writes the status file first; the WSL binary gets a runner choice (`OPREASON_RUNNER`) and the
fallback rule. `op.exe` runs in a kill-on-close Job Object.

**Tech Stack:** Rust (core, WSL binary, exe with the `windows` crate), bash black-box tests, Windows integration tests
with the fake `op.exe`, GitHub Actions, PowerShell 7 scripts on the Windows runner.

**Spec:** `docs/superpowers/specs/2026-11-15-wsl-through-exe-design.md`

## Global Constraints

- The payload and status fields of the A' spec §5 do not change, except the new status field `runner`.
- `op.exe` must never run twice for one call: the fallback runs only when no status file exists.
- Log records stay `v: 1`; new fields are `end.runner` and `end.fallback`; new refusal `no-exe`.
- Window texts: Rust's table and `launcher.ps1`'s `Get-Texts` stay equal (test).
- Tests never start a real `op.exe` or 1Password; the agent never starts the built exe locally.
- Dependencies: no new crates (the `windows` crate gets the `Win32_System_JobObjects` feature).

## Review Focus

1. The fallback cannot run `op.exe` twice (Task 2 and Task 3 tests count `op.exe` starts).
2. The exe writes the status file before anything else and stops when it cannot (Task 3 tests).
3. Killing the exe ends `op.exe` (Task 3 test).
4. stdin reaches the launcher intact after a fallback, also when it was read up front (Task 2 test).

---

### Task 1: Core

**Files:** Delete `crates/op-reason-core/src/launcher.rs`; modify `contract.rs`, `lib.rs`, `log.rs`, `logview.rs`,
`messages.rs`, `lang.rs`.

- [ ] Remove `ENV_SCRIPT`, `SCRIPT_B64_MAX` and the launcher module.
- [ ] `log::End` gets `runner` and `fallback` (skipped when `None`); `log::Fallback { why, exit }`.
- [ ] The log viewer marks `(fallback)`; `no-exe` gets a label.
- [ ] Messages: `no_exe`, `no_launcher`, `runner_fallback`, `no_fallback`; usage without "a fresh powershell.exe".
- [ ] `Lang::from_code`. Unit tests; `cargo test --workspace`.

### Task 2: WSL binary

**Files:** Create `crates/op-reason/src/runner.rs`; modify `wrapper.rs`, `main.rs`; modify `tests/wrapper.test.sh`,
`tests/e2e.test.sh`, `tests/launcher.test.sh` (comments).

- [ ] `runner.rs`: `OPREASON_RUNNER`, finding `op-reason.exe`, `launcher.ps1`, `powershell.exe`, the plan and its
  refusals, the command lines (`--run`; `-File <launcher.ps1>`).
- [ ] `wrapper.rs`: start the planned runner, the fallback when no status file exists (stderr line, `end` fields, the
  drained stdin again), `notify` through the runner.
- [ ] Black-box tests: one fake runner script installed as `op-reason.exe` and `powershell.exe`; the fallback matrix;
  `op.exe` starts counted; e2e through `-File` and an `auto` fallback to the real launcher.
- [ ] `bash tests/run-all.sh`, shellcheck, shfmt.

### Task 3: op-reason.exe --run and the Job Object

**Files:** Create `crates/op-reason-win/src/payload.rs`, `src/win/run.rs`, `src/win/job.rs`; modify `src/ui.rs`,
`src/lib.rs`, `src/win/app.rs`, `src/win/opexe.rs`, `src/win/mutex.rs`, `src/win/mod.rs`, `Cargo.toml`,
`fake-op/src/main.rs`, `tests/exe.rs`.

- [ ] `payload.rs` (portable, unit-tested on Linux): read the payload leniently, the status JSON, `args64`.
- [ ] `ui.rs`: the notify texts and `build_notify`, the status `ui` JSON; the launcher table test covers them.
- [ ] `job.rs`: create the job, start `op.exe` suspended, assign, resume, release, terminate.
- [ ] `run.rs`: the runner face (§5 of the spec); `app.rs` dispatches `--run`.
- [ ] Windows tests: every mode, first write, lock time-out, `lock: skipped`, bad payloads, unwritable status, Job
  Object in both faces. Local: `cargo clippy --target x86_64-pc-windows-msvc`, `cargo test` on Linux.

### Task 4: CI and release

**Files:** Rename `.github/scripts/windows-exe-dist.ps1` → `windows-dist.ps1`; modify `.github/workflows/release.yml`,
`.github/workflows/windows-exe.yml`, `.github/scripts/windows-exe-shots.ps1`, `flake.nix`.

- [ ] The dist script makes `op-reason-windows.zip` (exe, launcher, entry, skill, READMEs, licenses) and its SHA-256.
- [ ] `release.yml`: one Windows job builds the exe and the zip; the separate exe asset goes away.
- [ ] `windows-exe.yml`: run the dist script, check the zip's files and hash, upload it.
- [ ] Screenshot script: a `--run` scene with herdr and notes, `--run` timings.
- [ ] `flake.nix`: drop `./launcher` from the package source.

### Task 5: Documents

**Files:** `README.md`, `README.ja.md`, `skill/SKILL.md`, `docs/architecture/README.md`,
`docs/architecture/plantuml/01_overview.puml` (+ SVG), `docs/manual-verification.md`, `docs/manual-verification.ja.md`,
`launcher/launcher.ps1` (comments), the A' and C' specs.

- [ ] How it works, install (one zip, `OPREASON_EXE`, `OPREASON_RUNNER`), exit codes, the fallback.
- [ ] Manual checks: time to the window from WSL, TERM and Ctrl+C from WSL, Smart App Control and the fallback.
- [ ] Specs: point the changed sections at the new spec.
