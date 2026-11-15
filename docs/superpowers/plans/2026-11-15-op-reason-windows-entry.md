# op-reason PR 2 (Windows entry and launcher lock) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Call op-reason from Windows itself through a PowerShell entry, make calls from WSL and Windows run one at a time
with a named mutex in the launcher, and run the launcher and the entry on real Windows PowerShell 5.1 in CI with a
screenshot of the window on the pull request.

**Architecture:** `windows/op-reason.ps1` plans the call with the same rules as the Rust core (the call table), writes
the same log under `%LOCALAPPDATA%\op-reason\`, and starts a fresh `powershell.exe` running `launcher/launcher.ps1`
with the payload. The launcher takes `Local\op-reason` (unless `lock: skipped`), writes `holder.json`, and, for the
Windows entry, relays and scans `op.exe`'s stderr (`stderr: scan`). `windows/op-reason.cmd` starts the entry for
programs that cannot run `.ps1` files.

**Tech Stack:** Windows PowerShell 5.1 (also runs on PowerShell 7), C# compiled by `Add-Type` for the fake `op.exe` in
CI, GitHub Actions (windows-2025), bash for the screenshot comment.

**Spec:** `docs/superpowers/specs/2026-11-15-op-reason-design.md` (sections 5, 7, 8)

## Global Constraints

- `.ps1` files: UTF-8 with BOM, Windows PowerShell 5.1 syntax (no `??`, `?.`, ternary, `&&`/`||` chains, `$IsWindows`).
  `.cmd` files: CRLF.
- Mutex `Local\op-reason`; holder file `%LOCALAPPDATA%\op-reason\holder.json` (`time`, `caller`, `reason`, `command`,
  `pid`); status adds `lock` (`held`, `skipped`, `timeout`), `holder`, `stderrMatch`.
- The log format, the call table and the exit codes are the ones of PR 1; a mutex timeout gives exit 75 and an `end`
  record with result `lock-timeout`.
- The launcher's `Get-Texts` keeps one `key = 'value'` per line.
- Environment: `OPREASON_LANG`, `OPREASON_LOCK_WAIT`, `OPREASON_OP_EXE`, `OPREASON_POWERSHELL`; the shim sets
  `OPREASON_ENTRY=cmd`.
- Tests never run a real `op.exe` or 1Password.

## Review Focus

1. Arguments through `op-reason.cmd` with spaces, quotes, empty strings and `--` arrive unchanged (the entry re-reads
   the command line instead of PowerShell's `-File` binding, which drops empty strings and `--`). — Task 3 test.
2. Binary stdin and stdout through `op-reason.cmd` stay byte for byte. — Task 3 test.
3. A held mutex makes the call wait, then exit 75 with the holder's reason; a `lock: skipped` call does not wait. —
   Task 2 and Task 3 tests.
4. Japanese text in the window and messages renders (no mojibake under a CP932 console). — Task 2 screenshot (ja and
   en).
5. The user's PowerShell session keeps its environment variables and console encoding after a call. — Task 3 test.

---

### Task 1: Launcher lock and stderr scan

**Files:** Modify `launcher/launcher.ps1`; extend `tests/launcher.test.sh` (Linux pwsh: `stderr: scan` relays and
reports `stderrMatch`; the mutex is skipped on Linux and status `lock` is `skipped`).

- [ ] Write the failing tests (scan: denial line → `stderrMatch` `denied`, the line still reaches stderr; timeout line →
  `timeout`; inherit mode → `stderrMatch` absent).
- [ ] Implement `Enter-GlobalLock`/`Exit-GlobalLock` (Windows only), `holder.json`, the stderr pump, status fields.
- [ ] Run `bash tests/launcher.test.sh` → pass. Commit `feat(launcher): one call at a time across WSL and Windows`.

### Task 2: Windows tests of the launcher

**Files:** Create `tests/windows/fake-op.cs`, `tests/windows/lib.ps1`, `tests/windows/launcher.test.ps1`.

- [ ] Tests on Windows PowerShell 5.1: tricky arguments round trip; binary stdin/stdout through `cmd /c` redirection;
  exit code; window `shown`; mutex held by another process → exit 75, status `lock` `timeout`, `holder` names the
  holder's reason; `lock: skipped` runs while the mutex is held and does not sign out; `stderr: scan` → `stderrMatch`;
  screenshots of the window (Japanese and English) into `screenshots/`.
- [ ] Commit `test(windows): run the launcher on Windows PowerShell 5.1`.

### Task 3: The Windows entry

**Files:** Create `windows/op-reason.ps1`, `windows/op-reason.cmd`, `tests/windows/entry.test.ps1`.

- [ ] Tests: the call table in PowerShell mode (`& op-reason.ps1 @argv`): usage/version/skill outputs, refusals (exit 2,
  `refused` record), runs (op receives `opArgs`; `start` has `mode`, `kind`, masked `args`, normalized `reason`; `end`
  `approved`/`ok`); the same through `op-reason.cmd` for the cases cmd.exe can carry; binary stdin/stdout through the
  shim; denial → `denied`; mutex held → exit 75 and `end` `lock-timeout`; the caller's environment and console encoding
  are restored.
- [ ] Implement the entry (planning, caller facts with the process chain, place from git files, log with `log.lock`,
  payload, starting the launcher natively in PowerShell and with inherited handles from the shim, result, `end`).
- [ ] Commit `feat(windows): add the PowerShell entry and the cmd shim`.

### Task 4: CI, screenshots on the PR, release zip, documents

**Files:** Create `.github/workflows/windows.yml`, `.github/workflows/screenshots-cleanup.yml`,
`.github/scripts/pr-screenshots.sh`; modify `.github/workflows/release.yml`, `README.md`, `README.ja.md`,
`docs/architecture/README.md`, `docs/manual-verification.md`, `docs/manual-verification.ja.md`, the design document.

- [ ] Windows job (tests, screenshots artifact), Linux job that comments the screenshots on the PR (sticky comment,
  images on `screenshots/pr-<n>`), cleanup on close, release job that builds `op-reason-windows.zip` with its SHA-256.
- [ ] Docs: install and use on Windows, the `.cmd` caveat (cmd.exe re-parses arguments), `'--'` in PowerShell, the
  execution policy, interrupted calls, Windows checks with the real 1Password app.
- [ ] Push, open the pull request on top of PR 1, confirm the jobs pass and the screenshot comment renders.
