# How op-reason works

This page describes op-reason as it is now. Why it is built this way is in the design documents
([docs/superpowers/specs/2026-11-15-op-reason-design.md](../superpowers/specs/2026-11-15-op-reason-design.md),
[2026-11-15-windows-exe-design.md](../superpowers/specs/2026-11-15-windows-exe-design.md) and
[2026-11-15-wsl-through-exe-design.md](../superpowers/specs/2026-11-15-wsl-through-exe-design.md)).

![op-reason components](plantuml/out/01_overview.svg)

## Parts

| Part | Where | Role |
| --- | --- | --- |
| WSL executable | `crates/op-reason/` | `op` / `op-reason`: the wrapper; `op-reason-log`: the log viewer. Linux only (`/proc`, flock, signals, `WSLENV`). Starts a runner for every call |
| Core library | `crates/op-reason-core/` | argument planning, the runner contract, log records and the log viewer, place detection, messages; builds for Windows too |
| Windows executable | `crates/op-reason-win/` | `op-reason.exe`, two faces: the Windows entry (the whole call in one process: Direct2D window, WinRT toast, `op.exe`, sign-out, log, `--log`) and the runner of calls from WSL (`--run`). A Cargo workspace of its own, on the core |
| Launcher | `launcher/launcher.ps1` | Windows PowerShell 5.1 script, the fallback runner: started fresh for a call from WSL when `op-reason.exe` cannot run, and for every call through the PowerShell entry: window, toast, `op.exe`, sign-out, status |
| PowerShell entry | `windows/` | `op-reason.ps1` and `op-reason.cmd` (a shim for cmd.exe and other programs): the fallback where Smart App Control blocks `op-reason.exe` |
| Fish completion | `completions/op.fish` | template printed by `op-reason --completion fish` |
| Skill | `skill/SKILL.md` | printed by `op-reason --skill` |
| Call table | `tests/cases/calls.json` | invocations and their expected plan, shared by the Rust tests and the Windows entry's tests |

## One call from WSL

1. **Plan** the arguments: the wrapper's own options, the reason (`--reason`, normalized), the mode (`desktop`, `help`,
   `service`), masking. No reason and not a help call → refused (exit 2).
2. **Collect** the caller: a `claude` ancestor in `/proc`, herdr workspace and pane, repository and branch read from the
   git files of the current directory, `CLAUDE_CODE_SESSION_ID`.
3. **Check** the tools (`op.exe`, the runner chosen by `OPREASON_RUNNER` below, `wslpath`; exit 127) and the size
   (arguments travel in a Windows environment variable; exit 2).
4. **Pipes**: if another op-reason feeds stdin, read stdin to the end first. So does a desktop `op inject` without
   `-i` when stdin is not a terminal (its template is on stdin): the runner gets the same bytes.
5. **Lock** `<state>/lock` (flock), waiting up to `OPREASON_LOCK_WAIT` seconds (exit 75). While waiting: drain stdin if
   another op-reason starts feeding it; skip the lock if the holder reads our output. `<state>/current.json` names the
   holder.
6. **Watchdog**: `op.exe whoami` straight from WSL. Success means a shared approval: `signout --all`, log, notify.
   A call that skipped the lock leaves the watchdog to the lock holder. The payload is built and its size checked
   here, after the lock (exit 2 when it is over 32000 characters), with the `op://` references the call reads
   (below).
7. **Start** record, then the runner with the payload in `OPREASON_PAYLOAD`: `op-reason.exe --run`, or the launcher
   (`powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -OutputFormat Text -File <launcher.ps1>`).
   stdin and stdout are the caller's; stderr is relayed while its tail is kept.
8. **Fallback** (`auto` only): when `op-reason.exe` left no status file (it could not be started, or it exited
   without writing one) and the call's run directory is still there, one stderr line, then the launcher with the same
   payload and stdin.
9. **Result** from the status file, `op`'s exit code and `op`'s stderr; watchdog again; **end** record (with `runner`,
   and `fallback` when step 8 ran); exit with `op`'s code.

Signals (INT, TERM, HUP) log `interrupted` (an `end` after the start, a `refused` before it) and exit 130, 143, 129.
Children start with an empty signal mask, and a TERM or HUP to the wrapper alone is forwarded to the runner's process
(on WSL, the interop process of the Windows program). Whether that ends `op-reason.exe` is a manual check; when the
exe ends, its Job Object ends `op.exe` and the prompt.

### Choosing the runner

| `OPREASON_RUNNER` | What runs |
| --- | --- |
| `auto` (default; also unset, empty or unknown) | `op-reason.exe`; the launcher when the exe left no status file. No exe at all: the launcher next to where `OPREASON_EXE` points (a fallback), else refused `no-exe` (127) |
| `exe` | `op-reason.exe` only (no exe: refused `no-exe`) |
| `launcher` | the launcher only (no `launcher.ps1`: `no-launcher`; no `powershell.exe`: `no-powershell`; both 127) |

`op-reason.exe` is `OPREASON_EXE` (a WSL path) or `op-reason.exe` in `PATH`; `launcher.ps1` is looked for in its
folder (the Windows zip keeps them together); `powershell.exe` is `OPREASON_POWERSHELL` or the one in `PATH`. The
fallback cannot run `op.exe` twice: the exe writes the status file before anything else and stops when it cannot (also
when the payload's status path is empty; the WSL executable refuses a call whose paths `wslpath -w` could not
convert).

## One call from Windows (op-reason.exe, the entry face)

`op-reason.exe` is a fresh process for every call, so the approval binds to it, and it exits after the call. It does
what the PowerShell entry and the launcher do, in one process:

1. **Plan** with the core's planning function (the WSL executable's): wrapper options, the reason, the mode, masking.
   No reason and not a help call → `refused` (exit 2).
2. **Find** `op.exe`: `OPREASON_OP_EXE`, else the first `op.exe` in an absolute `PATH` entry that is not this exe
   (exit 127). Refuse when `op.exe`'s command line would be over 32,766 characters (exit 2).
3. **Collect** the caller (the PowerShell entry's rules; the process chain from one Toolhelp snapshot) and the place
   from git files.
4. **Lock** `Local\op-reason` before the `start` record, waiting up to `OPREASON_LOCK_WAIT` seconds (a time-out is a
   `refused` record with `lock-timeout`, exit 75); write `holder.json`. A mutex that cannot be used at all is recorded
   as `lock: failed`, and the call goes on.
5. **Start** record; the window (Direct2D and DirectWrite, the launcher's texts) and the toast (on a worker thread);
   `op.exe` with the caller's stdin and stdout, its stderr copied through while the tail is kept for `denied` and
   `timeout`. A desktop `op inject` without `-i` reads its template from stdin before the lock (when stdin is not a
   console), and `op.exe` gets the same bytes. `op.exe` runs in a kill-on-close Job Object (created suspended, put in
   the job, resumed): if the exe dies, `op.exe` ends too; when `op.exe` ends by itself the flag is cleared.
6. **Close** the session (`whoami --format json`, then `signout --all` when signed in; not for service calls; the
   account ID and host of a whoami that succeeded go to `account.json`), **end** record, remove `holder.json`, release
   the mutex and exit with `op`'s code. Ctrl+C: `interrupted`, 130; the console closed: 129.

The window and toast texts are a Rust copy of the launcher's table (`crates/op-reason-win/src/ui.rs`). A test reads
`Get-Texts` out of `launcher.ps1` and fails when the two differ: change both together.

## One call from WSL in op-reason.exe (the runner face)

`op-reason.exe --run` follows the runner contract below, as the launcher does:

1. Read `OPREASON_PAYLOAD` and, before anything else, write the status file: `{"started":false,"error":"","runner":"exe"}`.
   When it cannot be written: one stderr line, exit 1, nothing started (the WSL executable may then start the
   launcher).
2. Check the payload (`v` 1, a known mode, base64 arguments; an error in the status and exit 1) and `op` (exit 127).
3. Take `Local\op-reason` unless `lock` is `skipped`, waiting up to `lockWait` seconds (after a second it says once on
   stderr that it waits); `holder.json` while it holds it; a time-out writes `lock-timeout` and exits 75.
4. `desktop`: the window (with the herdr line, the notes and the references of the facts) and the toast; `op.exe` in
   the Job Object with the exe's stdin, stdout and stderr (or a scanned stderr with `stderr: scan`); `whoami --format
   json` (its account goes to `account.json`) and `signout --all` unless `lock` is `skipped`. `help` and `service`:
   `op.exe` without a window. `notify`: the toast only.
5. Release the mutex, write the whole status (with `runner: "exe"` and `ui`), exit with `op`'s code. No log record:
   the WSL executable writes them.

## One call through the PowerShell entry (fallback)

1. **Arguments**: from PowerShell, PowerShell's arguments (`'--'` must be quoted); through `op-reason.cmd`, the entry
   splits the process command line itself (empty arguments and `--` survive).
2. **Plan** with the same rules as the core (the call table runs against both).
3. **Collect** the caller (Claude Code from `CLAUDECODE`/`CLAUDE_CODE_SESSION_ID`/`CLAUDE_PID`, Windows Terminal from
   `WT_SESSION`, the process chain, herdr), the place from git files, and log to `%LOCALAPPDATA%\op-reason\log.jsonl`
   (the same records; appends lock `log.lock`).
4. **Launcher**: a fresh `powershell.exe -File launcher.ps1` with the payload (`lock: held`, `lockWait` =
   `OPREASON_LOCK_WAIT`, `stderr: scan`). The launcher takes `Local\op-reason`, writes `holder.json`, relays `op.exe`'s
   stderr and reports a denial or timeout in `stderrMatch`. From PowerShell the launcher runs as a native command (output
   can be captured); through the shim it inherits the handles (binary-safe).
5. **Result** and **end** record; the entry puts its environment changes and the console encoding back, and exits with
   `op`'s code (75 and an `end` with `lock-timeout` when the mutex wait runs out).

There is no shared-approval watchdog on Windows (no shared parent process).

## The references a call reads

Desktop calls list the `op://` references they read in the window, after the command line
([2026-11-17-refs-in-window-design.md](../superpowers/specs/2026-11-17-refs-in-window-design.md)):

- **Sources**: the arguments before `--` that start with `op://`; the template of `op inject` (its `-i` file, else
  stdin: the WSL executable and `op-reason.exe` read it when stdin is not a terminal; the PowerShell entry never reads
  stdin); the `--env-file`s of `op run`. Files are read relative to the entry's current directory (the first 8 MiB).
  The core collector (`crates/op-reason-core/src/refs.rs`) follows 1Password's template syntax; the PowerShell entry
  implements the same rules, and `tests/cases/refs.json` runs against both.
- **Window**: 「読む参照 (N 件・… から)」 / "References to read (N, from …)", at most 8 references, then "and N more". A
  reference whose vault and item are IDs (26 lowercase letters or digits) is a link to
  `onepassword://view-item?a=<account>&v=<vault>&i=<item>`; a click (press and release) opens the item in the
  1Password app without activating the window (`op-reason.exe`: `ShellExecuteW` on a thread of its own; the launcher:
  `Process.Start`; the Windows shell hands the handler `…view-item/?a=…`). Until an account is known, one line says
  why there are no links.
- **Account copy**: `%LOCALAPPDATA%\op-reason\account.json` (`{"v":1,"accounts":[{"id":…,"host":…}]}`, the last one seen
  first, at most 5), written by the runners after the end-of-call `whoami --format json` succeeded: its `account_uuid`
  and the host of its `url`, nothing else. The call's `--account` chooses among them (ID, host, or the host's first
  label); without it, the first.

## Language

English by default. The language chosen in the window of `op-reason.exe` comes first, then `OPREASON_LANG`; `LANG`
and the Windows display language do not count
([2026-11-15-language-toggle-design.md](../superpowers/specs/2026-11-15-language-toggle-design.md)).

- The window's switch (`English | 日本語`, at the right of the footer) lays the window out again in the other language
  and writes `%LOCALAPPDATA%\op-reason\settings.json` (`{"v":1,"lang":"en"|"ja"}`, through a temporary file). The
  window stays inactive: the 1Password prompt keeps the focus.
- `op-reason.exe` (both faces), the launcher and the PowerShell entry read it; a missing or broken file means nothing
  is chosen. The runners prefer it over the payload's `lang`, and write what it holds into the status as `chosenLang`.
- The WSL executable cannot read `%LOCALAPPDATA%`: it keeps a copy, `<state>/lang.json`, from each status's
  `chosenLang` (written for `en`/`ja`, removed for null, untouched when missing), and uses it before `OPREASON_LANG`
  for its messages, the payload's `lang` and `op-reason-log`.
- A call keeps the language it started with for its own messages; a click takes effect in the window at once and in
  every later call.

## The runner contract

The entries that start a runner (the WSL executable and the PowerShell entry) and the runners (`op-reason.exe --run`
and the launcher) agree on the following. Changing any of it is a contract change: update every side,
`tests/e2e.test.sh`, `crates/op-reason-win/tests/exe.rs` and this page. The entry face of `op-reason.exe` starts no
runner, but it shares the lock and `holder.json`, the log and the window texts.

### Environment

| Variable | Content |
| --- | --- |
| `OPREASON_PAYLOAD` | the payload JSON, ASCII only (`\u` escapes); `op.exe` never gets it |
| `WSLENV` | WSL only: the caller's value plus `OPREASON_PAYLOAD`; service calls add `OP_SERVICE_ACCOUNT_TOKEN/w` or `OP_CONNECT_HOST/w:OP_CONNECT_TOKEN/w`, and `OP_BIOMETRIC_UNLOCK_ENABLED/w` (set to `false`) |

The runner's command line: `op-reason.exe --run` (one fixed argument), or `powershell.exe -NoProfile
-NonInteractive -ExecutionPolicy Bypass -OutputFormat Text -File <launcher.ps1>`.

### Payload

| Field | Values |
| --- | --- |
| `v` | `1` |
| `mode` | `desktop`, `help`, `service`, `notify` |
| `op` | Windows path of `op.exe` |
| `args64` | `op`'s arguments, each UTF-8 then base64 |
| `status` | path for the status JSON |
| `lang` | `en`, `ja`: what the entry decided (the runner prefers the language chosen in the window) |
| `lock` | `held` (take the Windows mutex) or `skipped` (do not, and do not sign out) |
| `lockWait` | seconds to wait for the mutex |
| `stderr` | `inherit`, or `scan` (the runner relays `op.exe`'s stderr and reports `stderrMatch`; used by the Windows entry) |
| `facts` | desktop only: `time`, `caller` (`type`, `pid`, `session`, `detail`), `herdr` (`workspace`, `label`, `pane`) or null, `place` (`repo`, `branch`, `cwd`), `reason` (≤ 200 characters), `command` (masked, ≤ 300), `notes` (`shared-cleared`; `lock-skipped` with `holderReason`), `refs` when the call reads `op://` references (`total`; `from`: `kind` `args`, `file`, `stdin` or `env-file`, and `name` for files; `list`: the first 8, ≤ 200 characters each; `account`: the call's `--account`) |
| `notify` | notify only: `kind` (`shared-approval`), `when` (`start`, `end`), `cleared`, `time` |

The runner builds every text of the window and the toast from `facts` and `notify`, in the language chosen in the
window (`settings.json`) or else in `lang`.

### Status (written by the runner)

`started`, `exit`, `error` (`lock-timeout` when the mutex wait ran out), `window` and `toast` (`shown`, `skipped`,
`failed: …`), `others` (other `op.exe` seen), `signout` (`ok`, `not-signed-in`, `skipped`, `failed: …`), `lock` (`held`,
`skipped`, `timeout`; `failed` from `op-reason.exe` when the mutex cannot be used), `holder` (one line, on a timeout),
`stderrMatch` (`denied`, `timeout`; scan mode), `runner` (`exe` from `op-reason.exe`; the launcher writes none), `ui`
(the texts it built; `refs` with `head`, `items` (`text`, `linked`), `more` and `hint` when the window lists
references), `chosenLang` (`en`, `ja`, or null: what `settings.json` holds when the status is written; missing
when the runner cannot tell). `op-reason.exe` writes the status twice: at once, and at the end. The runner exits with `op`'s
code, 127 when `op.exe` is missing, 75 when the mutex wait runs out, 1 for other failures before `op.exe` starts, 0 for
`notify`.

### Lock

- WSL: flock on `<state>/lock`.
- Windows: the named mutex `Local\op-reason`, taken by the runner (desktop, help and service calls) unless `lock` is
  `skipped`, waiting up to `lockWait` seconds; `%LOCALAPPDATA%\op-reason\holder.json` names the holder (`time`,
  `caller`, `reason`, `command`, `pid`). A WSL call holds the flock and then the mutex; it passes what is left of its own
  wait as `lockWait`.
- `op-reason.exe` takes the same mutex around its whole call, before its `start` record, and writes `holder.json` the
  same way.

## Log

`${XDG_STATE_HOME:-~/.local/state}/op-reason/log.jsonl` on WSL (`%LOCALAPPDATA%\op-reason\log.jsonl` on Windows), one
JSON object per line, renamed to `log.1.jsonl` at 1 MiB. Appends take `log.lock`.

| `event` | Fields |
| --- | --- |
| `start` | `v`, `id`, `time`, `epoch`, `session`, `mode`, `kind` (`desktop`, `help`, `service-account`, `connect`), `lock` (`held`, `skipped`; `failed` from `op-reason.exe` when the mutex cannot be used), `caller` (`type`, `claude_pid`, `herdr`, `cwd`, `repo`, `branch`), `reason` (≤ 500), `args` (masked), `refs` when the call reads `op://` references (`total`, `from`, `list`: all of them, up to 1000, ≤ 300 characters each) |
| `end` | `v`, `id`, `time`, `epoch`, `session`, `result` (`approved`, `ok`, `denied`, `timeout`, `failed`, `launcher-failed` (the runner did not start `op.exe`), `interrupted`, `lock-timeout`), `exit`, `seconds`, `window`, `toast`, `others`, `signout`, `watch` (`start`, `end`: `none`, `cleared`, `error` or null); WSL calls add `runner` (`exe`, `launcher`) and, when `auto` fell back, `fallback` (`why`: `not-found`, `not-started`, `no-status`; `exit`: the exe's code or null) |
| `refused` | `v`, `id`, `time`, `epoch`, `session`, `why` (`no-reason`, `too-long`, `lock-timeout`, `no-op-exe`, `no-exe`, `no-launcher`, `no-powershell`, `no-wslpath`, `interrupted`), `caller`, `reason`, `args` |
| `shared-approval` | `v`, `id`, `time`, `epoch`, `when` (`start`, `end`), `cleared` |

A call's `start` and `end` share `id`. Denied and timed-out approvals are recognized from `op`'s own `[ERROR]` lines
(`authorization prompt dismissed`, `authoriz… timeout` / `timed out`).

## State directory

`<state>` = `${XDG_STATE_HOME:-~/.local/state}/op-reason/` (mode 0700): `log.jsonl`, `log.1.jsonl`, `log.lock`, `lock`,
`current.json` (while a call holds the lock), `run/<id>/status.json` and `run/<id>/notify-<when>.json` (during a call;
the runner writes them through the `\\wsl.localhost\…` path; leftovers older than an hour are removed unless the
process of their call still runs), `lang.json` (the copy of the language chosen in the window). Drained stdin and the
stderr tail stay in memory.

On Windows, `%LOCALAPPDATA%\op-reason\` holds the log (`log.jsonl`, `log.1.jsonl`, `log.lock`), `holder.json` (while a
call holds the mutex), `settings.json` (the language chosen in the window), `account.json` (the account IDs and hosts
that link references to the 1Password app) and, for calls through the PowerShell entry, `run/<id>/status.json`.

## Tests

| Test | What it covers |
| --- | --- |
| `cargo test --workspace` | the core (including the call table and the reference table) and the WSL helpers |
| `tests/wrapper.test.sh` | the executable with fake `op-reason.exe`, `powershell.exe`, `op.exe`, `wslpath` and `herdr`, including the runner choice and every fallback case (`op.exe` counted: never twice) |
| `tests/launcher.test.sh` | the launcher on PowerShell 7 for Linux with a fake `op` |
| `tests/e2e.test.sh` | the executable and the real launcher together (`-File`), and a fallback from an exe that writes nothing |
| `tests/log.test.sh` | the log viewer |
| `tests/completion.test.sh` | the fish completion with the real fish |
| `tests/windows/entry-plan.test.ps1` | the call table and the reference table against the Windows entry (Windows PowerShell 5.1 in CI, PowerShell 7 on Linux) |
| `tests/windows/launcher.test.ps1` | the launcher on Windows PowerShell 5.1 with a fake `op.exe` (arguments, binary stdin, the mutex, stderr scan, the references and the account copy, the window and its screenshots; on CI, a real click on a link with a temporary `onepassword:` handler) |
| `tests/windows/entry.test.ps1` | `op-reason.ps1` and `op-reason.cmd` end to end on Windows PowerShell 5.1 |
| `cargo test` in `crates/op-reason-win` (Linux) | the exe's window and toast texts against `launcher.ps1` (the block of references too), its usage, the payload and status of `--run`, the command-line length, the account copy and the item URL |
| `crates/op-reason-win/tests/exe.rs` (Windows) | `op-reason.exe` end to end with a fake `op.exe`: arguments, stdin, stderr, exit codes, the mutex and the holder, refusals and records, the window, `--` from PowerShell, the log viewer; `--run` in every mode (the first write, the lock time-out, `lock: skipped`, bad payloads, an unwritable status file); killing the exe ends `op.exe` (Job Object); the chosen language in both faces and `chosenLang`; the references (`ui.refs`, a template on stdin, files), the account copy from whoami; on CI, a real click on the language switch (the caption switches, `settings.json` is written, the focus stays) and on a link (a temporary `onepassword:` handler gets the item's URL, the window stays inactive) |
| `.github/scripts/windows-exe-shots.ps1` | screenshots and timings of `op-reason.exe` (both faces) and the PowerShell entry on the Windows runner |
| `.github/scripts/windows-dist.ps1` | the Windows zip of the release and its entries |
| `tests/release-notes.test.sh` | `.github/scripts/release-notes.sh` (the notes of a release: the version's section of `CHANGELOG.md`; the versions of `Cargo.toml`, `crates/op-reason-win` and the PowerShell entry agree, also with the tag) on made-up repositories and on this one |
| [manual-verification.md](../manual-verification.md) | checks with the real 1Password app (by hand) |
