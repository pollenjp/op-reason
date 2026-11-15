# op-reason: calls from WSL through op-reason.exe — design

- Date: 2026-11-15
- Status: approved (the design was agreed before this document was written; this document records it and the
  implementer's decisions)
- Builds on: `2026-11-15-op-reason-design.md` ("the A' spec": the WSL binary, the payload and status contract, the
  launcher, the PowerShell entry) and `2026-11-15-windows-exe-design.md` ("the C' spec": `op-reason.exe` as the
  Windows entry).

## 1. Goal and non-goals

Goal: the window of a call from WSL is shown by the same Rust `op-reason.exe` that serves calls from Windows, and
the PowerShell launcher becomes the way out when the exe cannot run (Smart App Control, for example).

```text
                 before                                         after
 WSL op-reason ──powershell.exe -EncodedCommand──▶ launcher    WSL op-reason ──op-reason.exe --run──▶ (window, op.exe)
 (launcher source in OPREASON_SCRIPT)                                         └─powershell.exe -File launcher.ps1
                                                                                (only when the exe left no trace)
```

- `op-reason.exe` gets a second face, the runner: `op-reason.exe --run` takes the call that the WSL binary
  describes in `OPREASON_PAYLOAD` (payload v1, A' spec §5.2) and does what `launcher.ps1` does with it: the
  window, the toast, the mutex, `op.exe`, the sign-out and the status file.
- The WSL binary chooses whom to start: `OPREASON_RUNNER=auto|exe|launcher` (§4).
- The launcher, `op-reason.ps1` and `op-reason.cmd` stay, unchanged in behavior, as the fallback.
- `OPREASON_SCRIPT` and everything it needed go away: the `-EncodedCommand` bootstrap, the launcher embedded in the
  WSL binary, the stripping of comment lines, and the 30,000-character limit of the launcher source (§8).

Non-goals:

- A window for Linux (the NixOS laptop). It is left for later; §11 says where it plugs in.
- A richer window (stage ④). It is designed and approved on its own later.
- One window implementation for every OS. Each OS keeps a window written the way that suits it; on Windows that is
  the Direct2D window of the C' spec (§7 there).
- Signing the exe, tags and releases.

## 2. What stays

- **The WSL binary** keeps everything that only WSL can see or do: the caller from the WSL process tree, herdr, the
  place, pipes between two calls, the flock, signals, the shared approval watchdog and the log.
- **The payload and the status file** (A' spec §5.2 and §5.4) stay as they are, so that the exe and the launcher take
  the same call and the WSL binary can switch to the launcher without building anything again. The payload still
  travels in `OPREASON_PAYLOAD` through `WSLENV`: it is a small ASCII JSON, far below the limit of an environment
  variable (32,767 characters; the WSL binary refuses a payload over 32,000).
- **The window texts** stay one table: Rust's table in `crates/op-reason-win/src/ui.rs` and the `Get-Texts` table of
  `launcher.ps1` are compared by a test (C' spec §11). The toast texts of the `notify` mode join that table (§7.5).
- **The exe's entry face** (`op-reason --reason "<why>" <op arguments…>` from Windows) is unchanged, except for the
  Job Object (§6).

## 3. One call from WSL

1. The WSL binary plans the call, takes the flock, runs the watchdog and writes `start`, as before (A' spec §6.1).
2. It starts the runner chosen in §4 with the payload in `OPREASON_PAYLOAD` (listed in `WSLENV`, plus the token
   variables with `/w` in service mode), its stdin and stdout, and a pipe for stderr that it relays and keeps the
   tail of. The exe gets one fixed argument, `--run`: WSL rebuilds the Windows command line of a Windows program
   from its arguments, and a fixed argument cannot be broken on the way.
3. The exe, as soon as it has read the payload, writes the status file (§5.1): this is the trace that it woke up.
4. The exe takes the mutex, shows the window and the toast, starts `op.exe`, signs out and writes the status again,
   with the result.
5. The WSL binary reads the status, classifies the result, runs the watchdog and writes `end`.

Once the exe has written the status file, the WSL binary never starts the launcher for that call, whatever the
status says: `op.exe` never runs twice for one call.

## 4. Choosing the runner (WSL)

### 4.1 Finding the parts

| Part | Where the WSL binary looks |
| --- | --- |
| `op-reason.exe` | `OPREASON_EXE` (a WSL path), else `op-reason.exe` in `PATH` (WSL adds the Windows `PATH` by default) |
| `launcher.ps1` | the folder of `op-reason.exe`: the folder of `OPREASON_EXE` (also when the exe itself is missing there), else the folder where `op-reason.exe` was found in `PATH` |
| `powershell.exe` | `OPREASON_POWERSHELL`, else `powershell.exe` in `PATH` (as before) |

The Windows zip (§9) puts `op-reason.exe` and `launcher.ps1` in one folder, so one folder in the Windows `PATH` serves
calls from Windows and from WSL.

### 4.2 Rules

`OPREASON_RUNNER` is `auto` (the default; also when unset, empty or not one of the three words), `exe` or
`launcher`.

| `OPREASON_RUNNER` | Found | What happens |
| --- | --- | --- |
| `auto` | `op-reason.exe` | Start the exe. When it leaves no status file, start the launcher with the same payload (§4.3). |
| `auto` | no exe, but `launcher.ps1` and `powershell.exe` | Start the launcher (a fallback: `not-found`). |
| `auto` | neither | Refuse with 127 (`no-exe`). |
| `exe` | `op-reason.exe` | Start the exe, never the launcher (when the exe leaves no status file, one line says so). |
| `exe` | no exe | Refuse with 127 (`no-exe`). |
| `launcher` | `launcher.ps1` and `powershell.exe` | Start the launcher, never the exe. |
| `launcher` | no `launcher.ps1` / no `powershell.exe` | Refuse with 127 (`no-launcher` / `no-powershell`). |

`op.exe` and `wslpath` are still required first (127, `no-op-exe` and `no-wslpath`).

### 4.3 The fallback

In `auto` mode the WSL binary starts the launcher after the exe only when the exe left **no status file at all**:

- the exe could not be started (`not-started`: starting it failed on the Linux side), or
- the exe ran and exited without writing the status file (`no-status`, with its exit code). Smart App Control
  refusing the exe, a broken interop, a payload the exe could not read, or a status path it could not write all end
  here.

Then the WSL binary:

- prints one line on stderr that says why and that `OPREASON_RUNNER=launcher` skips the exe;
- records the fallback in the `end` record (§10);
- starts `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -OutputFormat Text -File <launcher.ps1>`
  with the same payload, the same environment and the same stdin: the inherited stdin as it is, or, when stdin was
  read up front (a pipe from another op-reason, A' spec §6.4), the same bytes again;
- uses the launcher's status, exit code and stderr for the result.

When the fallback is needed but `launcher.ps1` or `powershell.exe` is missing, the call ends with `launcher-failed`
and 127, and the message names what is missing.

Why `op.exe` cannot run twice: the exe writes the status file before it does anything else (§5.1), and it stops
without starting `op.exe` when it cannot write it. A status file therefore exists whenever the exe may have started
`op.exe`, and the fallback runs only when none exists.

Known edge: when the exe starts and dies before its first write, while stdin is a pipe that WSL's interop already
began to read for it, the bytes it read are lost for the launcher. The exe's first write comes before anything else,
and when Windows refuses to start an exe, no relay starts (to be confirmed by hand, §13).

### 4.4 The shared approval toast (`notify`)

The watchdog's toast (A' spec §6.5) goes to the same runner:

- `auto`: the exe with a status path (`run/<id>/notify-<when>.json`); the WSL binary waits until that file exists or
  the exe exits (up to 5 seconds). When the exe exits without writing it, the launcher is started in the background,
  as before.
- `exe`, `launcher`, or `auto` without the exe: that runner in the background, as before.

### 4.5 The launcher started with `-File`

The launcher is started with `-File` from WSL too, as the PowerShell entry already starts it. An unsigned local `.ps1`
runs with `-ExecutionPolicy Bypass` (checked on 2026-11-14 on the author's machine, also with the Mark of the Web;
A' spec §1.2). The launcher sets `$ProgressPreference` in its first line, so PowerShell writes no progress records to
stderr.

## 5. The runner face of op-reason.exe

`op-reason.exe --run` (`--run` as the only argument) follows the launcher's contract (A' spec §5.1–§5.5). Extra
arguments after `--run` give exit 2. The exe writes no log record in this face: the WSL binary writes them.

### 5.1 The first write

1. Read `OPREASON_PAYLOAD` and remove it from the environment (`op.exe` must not inherit it).
2. Read the `status` path from the payload, even when the rest of it cannot be used.
3. Write the status file at once: `{"started":false,"error":"","runner":"exe"}`.
4. If the payload is empty or not JSON, or the status file cannot be written: print one `op-reason: …` line on
   stderr and exit 1 without doing anything else. A payload without `status` (allowed by the contract; the WSL binary
   always sends one, except for the toast of `OPREASON_RUNNER=exe`) runs without the first write.

### 5.2 Checks

- `v` must be 1. Another version is written to the status as an error ("update op-reason on both sides") with exit 1:
  the WSL binary and the Windows zip are installed separately and can drift apart.
- `mode` must be `desktop`, `help`, `service` or `notify`; `args64` must be base64. Otherwise: the error in the
  status, one stderr line, exit 1.
- `op` must be a file, or: status error `op.exe not found: <op>`, one stderr line, exit 127.

### 5.3 What each mode does

The same as the launcher (A' spec §5.3):

| Mode | Lock | Window and toast | `op.exe` | After `op.exe` |
| --- | --- | --- | --- | --- |
| `desktop` | the mutex, unless `lock: skipped` | yes | yes | `whoami`, then `signout --all` when signed in; nothing when `lock: skipped` |
| `help` | as above | no | yes | as above |
| `service` | as above | no | yes | nothing |
| `notify` | no | toast only | no | nothing; exit 0 |

- **Lock**: `Local\op-reason`, waiting up to `lockWait` seconds; an abandoned mutex counts as taken. After a second of
  waiting it says once on stderr that it waits (the launcher does the same). `holder.json` is written while it holds
  the mutex, with the launcher's fields and rules. On time-out: status `{"started":false,"error":"lock-timeout",
  "lock":"timeout","holder":"<one line>"}`, exit 75.
- **Arguments**: `args64` decoded as UTF-8 (invalid bytes become U+FFFD, as in the launcher), passed to `op.exe` one by
  one by Rust's quoting.
- **Environment**: the exe's own, minus `OPREASON_PAYLOAD`. In service mode the WSL binary has already passed the token
  variables; in the other modes it has removed them.
- **stdin and stdout** are the exe's own handles, passed to `op.exe` untouched.
- **stderr**: `inherit` (from WSL) gives `op.exe` the exe's stderr; `scan` relays it through a pipe and reports
  `stderrMatch` (`denied`, `timeout` or null), as the launcher does.
- **Window and toast**: the C' window and toast, built from `facts` with the shared table (herdr line and notes
  included), refreshed every 250 ms with the count of other `op.exe` processes.

### 5.4 The status at the end

The launcher's fields, plus `runner`:

```json
{"started":true,"exit":0,"error":"","window":"shown","toast":"shown","others":0,"signout":"ok","lock":"held","runner":"exe","ui":{…}}
```

`ui` has the launcher's shape: `title`, `lines` (pairs of label and value), `footer`, `others` and `toast` for a
desktop call; `title`, `lines` and `toast` for a notify call. The exit code is `op.exe`'s; 127, 75 and 1 as above;
`notify` exits 0.

### 5.5 Ctrl+C

As in the entry face (C' spec §10): `op.exe` gets two seconds to end by itself and is then terminated; the session is
closed (unless `lock: skipped`), the status says `error: interrupted`, and the exe exits with 130.

## 6. Job Object

`op.exe` runs in a Job Object that kills its processes when the job's last handle closes
(`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). Only the exe holds that handle, so when the exe dies (killed, or its console
closed), `op.exe` and its children end too, and so does their approval prompt. In both faces of the exe.

- `op.exe` is created suspended, put in the job, and then resumed, so that nothing it starts can escape the job.
- When `op.exe` ends by itself, the exe clears the kill-on-close flag before it goes on: a program that `op run`
  left running keeps running, as it does under the launcher.
- When the exe stops `op.exe` (Ctrl+C), it terminates the whole job.
- If the job cannot be used (an old Windows, or a job of a parent that does not allow nesting), `op.exe` runs without
  it.

## 7. Stopping a call from WSL

- The WSL binary still handles SIGINT, SIGTERM and SIGHUP (A' spec §6.1, step 7) and forwards TERM and HUP to the
  runner's process, which on WSL is the interop process that stands for the Windows program.
- Whether and how that reaches `op-reason.exe` (is it terminated, does it get a console event) is not known yet and is
  part of the manual checks (§13). When it terminates the exe, the Job Object (§6) ends `op.exe` and its prompt with
  it; the launcher could leave `op.exe` and the prompt behind.
- A Ctrl+C in a WSL terminal reaches the WSL binary as SIGINT; whether the Windows side gets a console event is also a
  manual check.

## 8. What goes away

- `OPREASON_SCRIPT`, the `-EncodedCommand` bootstrap, `op_reason_core::launcher` (the embedded launcher source, the
  stripping of comment and blank lines, the tests that kept it under 30,000 characters and free of here-strings),
  and `SCRIPT_B64_MAX`.
- The WSL binary no longer contains the launcher; the Nix package no longer needs `launcher/`.
- `launcher.ps1` itself changes only in its comments: it is started with `-File` by both callers.

The payload limit (32,000 characters) and the argument limit (28,000 characters of base64) stay: the payload is still
an environment variable.

## 9. The Windows zip

One release asset for Windows, `op-reason-windows.zip` (with `op-reason-windows.zip.sha256`), holds one folder:

```text
op-reason-windows/
  op-reason.exe        the Windows entry and the runner of WSL calls (not signed)
  launcher.ps1         the fallback of WSL calls (looked for next to op-reason.exe)
  SKILL.md  README.md  README.ja.md  LICENSE-MIT  LICENSE-APACHE
  powershell-entry/    the PowerShell entry, for Windows where Smart App Control blocks the exe
    op-reason.ps1  op-reason.cmd  launcher.ps1  SKILL.md
```

The user unpacks it and adds the folder to the Windows user `PATH`; where Smart App Control blocks the exe, also
`powershell-entry`, in front of it. The PowerShell entry has a folder of its own because PowerShell prefers a `.ps1`
to an `.exe` of the same name in one folder: `op-reason` typed in PowerShell would run the fallback (or fail under the
default execution policy, like the well-known `npm.ps1`). The entry finds `launcher.ps1` and `SKILL.md` next to
itself, so it keeps copies of them, and its code does not change.

The separate `op-reason.exe` asset of the C' spec goes away. The script that makes the zip checks its entries and runs
in the pull request workflow too (it is the step that a tag would otherwise test first).

Versions: the WSL binary and the zip are installed separately. The payload's `v` is the contract between them: an exe
that gets another version refuses it with a message to update both sides (§5.2).

## 10. Records and messages

- `end` gets `runner`: `exe` or `launcher`, whichever ran the call (missing when no runner ran, and on Windows).
- `end` gets `fallback` only when `auto` moved to the launcher: `{"why":"not-found"|"not-started"|"no-status",
  "exit":<the exe's exit code, or null>}`.
- `refused.why` gets `no-exe`; `no-launcher` (used by the bash version) and `no-powershell` keep their meaning.
- The log viewer marks a call that fell back with `(fallback)` (Japanese: `(回避経路)`).
- New messages (English and Japanese): `op-reason.exe` not found, `launcher.ps1` not found, the fallback line, and the
  fallback that cannot run. The usage no longer says that every call starts `powershell.exe`.

## 11. Linux later

The runner choice of §4 is also where a Linux version plugs in: a runner that shows the window and starts `op` in the
WSL binary's own process. The caller, the place, pipes, the flock and the log are Linux code already. What the
laptop needs (how 1Password binds an approval on Linux, the name of the setgid `op`, GNOME's limits on window
placement) is checked first, on the laptop. Splitting the flow of one call into a shared part waits until then.

## 12. Tests

- Linux, `tests/wrapper.test.sh` with a fake `op-reason.exe` (the fake runner writes the status first, as the exe
  does) and a fake `powershell.exe`:
  - the default runs the exe with `--run`; `OPREASON_RUNNER=launcher` runs `powershell.exe -File` with the Windows path
    of `launcher.ps1`; `exe` never falls back;
  - an exe that writes no status → the launcher runs once, with the stderr line and the `end` fields; an exe that
    writes the status and then fails, or that dies after `op.exe`, → no launcher;
  - an exe that cannot be started, an exe that is missing next to `launcher.ps1`, nothing at all (127), a missing
    `launcher.ps1` or `powershell.exe`, a fallback without a launcher;
  - `op.exe` runs exactly once in every case, also for a pipe from another op-reason whose bytes go to the launcher
    again;
  - the `notify` toast falls back the same way.
- Linux, `tests/e2e.test.sh`: the WSL binary with the real launcher through `-File` (fake `powershell.exe` running
  pwsh), and an `auto` call whose exe writes no status, ending in the real launcher.
- Windows, `crates/op-reason-win/tests/exe.rs` with the fake `op.exe`: `--run` in every mode (status fields, `ui`,
  stdin and stdout, stderr inherit and scan, the payload not inherited by `op.exe`), the first write seen while the
  mutex is busy, the time-out with the holder, `lock: skipped`, bad payloads (no `op.exe` started), an unwritable
  status path (no `op.exe` started), and the Job Object (killing the exe ends `op.exe`, in both faces).
- Windows, the screenshot job: a `--run` scene with the facts a WSL call sends (herdr line and notes), and timings of
  `--run` next to the entry face.

## 13. Not verified yet (manual checks)

On a Windows machine with WSL (the CI runners have no WSL2 and no 1Password), `docs/manual-verification.md`:

- how long a call from WSL takes until the exe's window is visible (from Windows it takes about 28 ms);
- what a TERM to the WSL binary does to `op-reason.exe` and `op.exe`, and what Ctrl+C in a WSL terminal does;
- what WSL shows when Smart App Control blocks `op-reason.exe`, and that the call falls back to the launcher with
  stdin intact.

## 14. Implementer's decisions

| Decision | Why |
| --- | --- |
| The exe's argument is `--run` alone; everything else comes in the payload | a fixed argument cannot be broken by the command line WSL builds |
| The first write is `{"started":false,"error":"","runner":"exe"}`, before the payload is checked | the trace must exist before anything with an effect, and a bad payload is better reported by the exe than retried |
| No status file can be written → exit 1 before anything else | keeps "a status file exists whenever `op.exe` may have run" |
| An unknown `OPREASON_RUNNER` means `auto` | the other `OPREASON_` variables fall back to their default too; `auto` still reaches the launcher |
| `launcher.ps1` is looked for in the exe's folder only (also the folder of an `OPREASON_EXE` whose exe is missing) | the zip keeps them together; no extra variable |
| An exe missing in `auto` mode falls back with the stderr line | the user learns why the window looks different and how to make it quiet |
| `notify` in `auto` waits up to 5 s for the exe's first write | the fallback needs to know whether the exe woke up; a shared approval is rare |
| `end.runner` always on WSL calls, `end.fallback` only when it happened | the log tells how often the exe is blocked without growing every line |
| `op.exe` created suspended, assigned, resumed; kill-on-close cleared when `op.exe` ends by itself | nothing escapes the job, and programs `op run` leaves behind behave as under the launcher |
| The Job Object in the entry face too | the same `op.exe` start code; a killed exe no longer leaves a prompt behind |
| The holder file follows the launcher's `Write-Holder` | one rule for both runners |
| One Windows zip, made by a script that the pull request workflow runs | one install for Windows and WSL; the release step is tested before a tag |
| The PowerShell entry in a `powershell-entry` folder of the zip, with copies of `launcher.ps1` and `SKILL.md` | PowerShell would pick `op-reason.ps1` over `op-reason.exe` in one folder; the entry's code stays as it is |
| The separate `op-reason.exe` release asset goes away | one way to install |
