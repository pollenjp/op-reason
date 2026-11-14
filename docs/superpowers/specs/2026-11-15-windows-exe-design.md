# op-reason on Windows as one Rust exe (plan C') — design

- Date: 2026-11-15
- Status: prototype for comparison. Not the default plan. The default Windows entry is plan A' (PowerShell entry +
  PowerShell launcher), described in `2026-11-15-op-reason-design.md` ("the A' spec" below).
- Scope: the Windows side only. The WSL side (the Rust binary, its flock and pipe handling, the embedded
  `launcher.ps1`) is the same in A' and C' and is not changed here.

## 1. Goal and non-goals

Goal: run every Windows-native call of op-reason through one native executable, `op-reason.exe`, and compare it
with plan A' on the same contract, so that the author can decide which Windows entry to ship.

```text
op-reason --reason "<why>" <op arguments...>
```

In A', that command is `op-reason.ps1` (or the `op-reason.cmd` shim), which starts a fresh `powershell.exe`
running `launcher.ps1` for every call. In C', the same command is `op-reason.exe`, and that one process does
everything the entry and the launcher do: it reads the reason, finds the caller and the place, takes the lock,
draws the window, shows the toast, starts `op.exe`, closes the CLI session and writes the record.

Non-goals (this PR):

- Calling `op-reason.exe` from WSL instead of `launcher.ps1`. Discussed in §12, not built.
- Signing the exe or applying to SignPath. §13 lists what an application needs.
- Making the window richer (stage ④). §7.4 says how the chosen window code serves as its base.
- Tags, GitHub Releases, crates.io.

## 2. Why the process boundary matters (facts this design keeps)

From the Windows-native check (2026-11-14, author's machine, Smart App Control on; A' spec §1.2):

- The 1Password desktop app binds a CLI approval to the process that starts `op.exe`. Two `op.exe` runs from
  the same parent share one approval; runs from different processes each need their own.
- The 1Password prompt names the requester "WindowsTerminal" whatever process asks, so a window that shows who is
  asking and why is needed on Windows as well.
- `op signout --all` closes the approvals of every process, not only the caller's (siblings, children, other
  terminal tabs).
- Windows PowerShell 5.1.26100, pwsh 7.7 for the Windows Claude Code tool shell, no `git.exe`, execution policy
  Restricted except `Process=Bypass` in the tool shell.

C' keeps the "one approval per call" property because `op-reason.exe` is itself a fresh process for every call:
the approval binds to it, and it exits after the call. No intermediate `powershell.exe` is needed.

## 3. Contract shared with A'

The A' spec (§4 command line, §5 contracts) is the source of truth. C' follows it; the table says where, and §15
lists every difference.

| Item | C' | A' spec |
| --- | --- | --- |
| Names, prefix | `op-reason.exe`, `OPREASON_` variables | §2 |
| Wrapper options | the same planning function as the WSL binary: `--help`/`-h` and `--version`/`-V` as the only argument, `--skill`, `--completion fish`, `--log`, usage without op arguments | §4.2 |
| Reason, mode, masking, display | the same core functions | §4.3–4.5 |
| Lock | named mutex `Local\op-reason` around the whole call, waiting up to `OPREASON_LOCK_WAIT` (default 90) seconds; abandoned counts as acquired; `holder.json` written while holding it | §5.5 |
| Lock skip | never on Windows (the pipe case is WSL only); `start.lock` is `held` | §5.5, §7 |
| Record | `%LOCALAPPDATA%\op-reason\log.jsonl`, `v: 1` `start`/`end`/`refused`, rotation at 1 MiB under `log.lock` | §5.6 |
| Result | `approved`, `ok`, `denied`, `timeout` (from op's `[ERROR]` lines), `failed`, `launcher-failed`, `interrupted` | §5.6 |
| Exit codes | op's code; 2; 75; 127; 1; 130/129 when interrupted | §5.7 |
| Language | `OPREASON_LANG`, then `LC_ALL`/`LC_MESSAGES`/`LANG`, then the Windows UI language | §5.8 |
| Window and toast text | the same texts as `launcher.ps1`'s table, built from the same facts | §5.2 `facts`, §8 |
| Caller, place | Claude Code, Windows Terminal tab, process chain; repo and branch from `.git` files | §6.6, §7 |
| Watchdog | none (no shared parent process on Windows) | §7 |

## 4. What op-reason.exe does (flow)

1. Plan the call with the core's planning function (A' spec §4): wrapper options print and exit; no op arguments
   print the usage; refusals are recorded (`refused`) and printed with the short usage (exit 2).
2. Find `op.exe` (§8.1); refuse with 127 when missing. Refuse with 2 (`too-long`) when the `op.exe` command line
   would not fit (§8.3).
3. Collect the caller and the place (§5).
4. Take the named mutex (§6). While waiting, say so once on stderr (only when stderr is a console). Refuse with 75
   (`refused`, `lock-timeout`, the core's mutex time-out message) at the deadline, naming the holder from
   `holder.json`. Write `holder.json`.
5. Write `start`.
6. `desktop` mode: show the window (§7), start the toast on a worker thread, then start `op.exe` (§8). Keep the
   window responsive until `op.exe` exits, refreshing the "other op.exe" line every 250 ms.
   `help` mode: start `op.exe` without a window. `service` mode: start `op.exe` with the token variables.
7. `desktop` and `help`: close the CLI session (§9).
8. Classify the result from the exit code and a copy of op's stderr, write `end`, remove `holder.json`, release the
   mutex, exit with op's exit code.

There is no status file and no payload: the facts that the A' entry passes to the launcher stay inside one
process, and the fields that the A' launcher writes to its status file go straight into the `end` record.

## 5. Caller and place

Caller (facts `caller.type`, `caller.pid`, `caller.session`, `caller.detail`), by the same rules as the A' entry:

- Claude Code when `CLAUDECODE=1` or `CLAUDE_CODE_SESSION_ID` is set, or a `claude.exe` is up the process tree. Its
  PID comes from `CLAUDE_PID`, else from the tree. The session id is `CLAUDE_CODE_SESSION_ID`.
- Otherwise a person.
- `detail` is the process chain (this process and up to five parents, `op-reason.exe ← pwsh.exe ← …`), followed by
  `Windows Terminal` when `WT_SESSION` is set.
- The tree comes from one Toolhelp snapshot (about a millisecond; the A' entry asks CIM). The walk stops at a parent
  created after its child, which means its PID was reused.

Place, without `git.exe` (A' spec §6.6): walk up from the current directory to the first `.git` (a `.git` file
means a worktree or a submodule, and `commondir` points at the shared directory), branch from `HEAD`, repository
from `[remote "origin"] url`, displayed as `owner/name (branch) · <cwd>` with the user profile directory as `~`.

## 6. Lock

- `CreateMutexW(Local\op-reason)` then `WaitForSingleObject` in 250 ms slices up to the deadline, so that Ctrl+C is
  noticed while waiting. `WAIT_ABANDONED` (the previous holder died) counts as acquired.
- The mutex lives in the session namespace (`Local\`). The `powershell.exe` that WSL starts runs in the same
  Windows logon session as Windows Terminal, so WSL calls (through the launcher) and Windows calls (this exe) see
  the same mutex and run one at a time.
- While holding it, the exe writes `%LOCALAPPDATA%\op-reason\holder.json` (time, short caller, reason, command, its
  own pid), as the launcher does, so that a call that times out can name the holder whichever side holds it.
- The lock is taken before `start` (like the WSL flock), so a time-out is a `refused` record. The A' launcher takes
  the mutex after the entry wrote `start`, so there a time-out is an `end` with `lock-timeout`. Both are values of
  the A' spec §5.6.
- If the mutex cannot be used at all (for example an elevated process created it with a restrictive DACL), the call
  runs without it and records `lock: failed` instead of blocking the user.

## 7. Window and toast

### 7.1 Options compared

| Option | Startup and size | Dependencies | Room for stage ④ | Accessibility and tests | Runners and VMs without a GPU |
| --- | --- | --- | --- | --- | --- |
| Win32 controls (STATIC, RichEdit) | fastest, smallest | windows crate | low: no anti-aliased shapes, monochrome emoji, fixed control look | good (MSAA/UIA proxies) | works |
| **Win32 window drawn with Direct2D and DirectWrite** (chosen) | fast, small | windows crate | high: anti-aliased shapes, color emoji, text layout with ranges, animation by timer, SVG icons through `ID2D1SvgDocument` | no UIA tree unless we add a provider | works (WARP fallback) |
| egui/eframe | a few MB, GPU init on start | about a hundred crates (wgpu) | high, with ready-made widgets | AccessKit | needs D3D12 or WARP; the OpenGL backend fails without a GPU driver |
| WebView2 (wry/tao, webview2-com) | slowest, extra `msedgewebview2.exe` processes | WebView2 runtime | highest (HTML/CSS) | full | needs the runtime |
| WinUI 3 / XAML | heavy | Windows App SDK runtime | high | full | needs the runtime |
| Toast only (WinRT) | fast | windows crate | low: a few text lines; buttons need COM activation | n/a | Server editions may not show it |

Why Direct2D/DirectWrite:

- One dependency family (Microsoft's `windows` crate) for the window, the toast, the mutex and the process
  calls. A wrapper that sits next to 1Password approvals should not pull in a GPU stack of a hundred crates.
- It renders on machines without a GPU (Hyper-V VMs, RDP sessions, GitHub's Windows runners) through WARP, where
  the OpenGL path of egui does not.
- It is the most flexible base for stage ④ short of a browser engine, while keeping a one-file exe that starts in
  tens of milliseconds.

What it costs: every widget is drawn by hand (layout, hit testing for any future button), the code uses `unsafe`
FFI, and there is no accessibility tree (screen readers read the window title only). §7.4 lists the follow-ups.

### 7.2 Window (same look and behavior as the A' launcher)

- Width 520 DIP, height fitted to the text; top right of the primary monitor's work area with a 16 DIP margin, so
  it does not cover the 1Password prompt in the middle of the screen.
- Always on top, does not take the focus (`WS_EX_NOACTIVATE`, `SW_SHOWNOACTIVATE`, `MA_NOACTIVATE` on clicks), not
  in the taskbar (`WS_EX_TOOLWINDOW`), not resizable, small caption with the title and a close button.
- Light yellow background; the title in bold 16 DIP; one line per fact as "label: value" with a bold label; the
  "other op.exe" line in bold dark orange, hidden until another `op.exe` runs; the footer in bold firebrick.
- Segoe UI with DirectWrite's system font fallback, using the user's locale so Japanese gets Japanese glyphs.
  Color emoji are drawn in color; if the driver refuses that, the window is redrawn without it.
- Per-monitor DPI aware (V2, from the manifest); a DPI change re-lays out the text.
- The window asks nothing; the decision is made in the 1Password prompt only. Closing it does not stop the call.

### 7.3 Toast

- WinRT `ToastNotificationManager` through the `windows` crate, `ToastGeneric` with the three text lines that the
  A' launcher shows.
- AppUserModelID: the one Windows PowerShell registers (the A' launcher uses the same), so nothing is registered.
  The toast therefore reads "Windows PowerShell".
- Runs on a worker thread so that `op.exe` (and its prompt) does not wait for the notification platform.

### 7.4 As the base for stage ④

- Possible without new dependencies: an icon per caller type, a progress or elapsed-time line, color for the
  caller and the reason, a countdown to the approval timeout, a button that stops the call (terminate `op.exe`),
  a list of calls waiting for the lock.
- To add with the first interactive element: a UI Automation provider (or at least `WM_GETOBJECT` with MSAA) so
  that screen readers and UI tests can read the text.
- Our own AppUserModelID (`HKCU\Software\Classes\AppUserModelId\op-reason`) would show the toast as "op-reason"
  with an icon, and is the step before toast buttons. It writes to the registry, so it should be opt-in.
- Caveat: as long as WSL calls use `launcher.ps1`, the WSL window is WPF and the Windows window is Direct2D. Any
  stage ④ change has to be made twice, or WSL has to move to the exe (§12).

## 8. Starting op.exe

### 8.1 Finding it

`OPREASON_OP_EXE` when set, else the first `op.exe` in an absolute `PATH` entry. Relative entries and the current
directory are skipped (Windows' own search would run an `op.exe` planted in the current directory). An `op.exe` that
is this exe (a copy named `op.exe` earlier in `PATH`) is skipped to avoid calling ourselves.

### 8.2 Standard streams, arguments and environment

- stdin and stdout are inherited handles: bytes reach `op.exe` untouched (no PowerShell pipeline, no re-encoding).
- stderr goes through a pipe that a thread copies to our stderr as it arrives, keeping a bounded copy to tell
  `denied` and `timeout` from `failed` (the A' launcher does the same with `stderr: scan`). `op.exe` therefore sees
  a pipe on stderr, as it does when called from WSL.
- Arguments are passed as `OsString`s through `std::process::Command`, which quotes them by the Microsoft C
  runtime rules that Go's `os.Args` parser (and so `op.exe`) reads back unchanged. Unpaired surrogates survive.
- The console stays shared and no window is hidden (`CREATE_NO_WINDOW` is not used), so `op.exe` keeps its stdin.
- Environment per kind, as on WSL: `desktop`/`help` remove `OP_SERVICE_ACCOUNT_TOKEN`, `OP_CONNECT_HOST`,
  `OP_CONNECT_TOKEN`; `service-account` keeps the token and sets `OP_BIOMETRIC_UNLOCK_ENABLED=false`; `connect`
  removes the service-account token and sets `OP_BIOMETRIC_UNLOCK_ENABLED=false`.

### 8.3 Length limit

`CreateProcessW` accepts a command line of up to 32,767 UTF-16 units. The exe computes the quoted length first
and refuses with 2 (`too-long`) instead of failing inside `CreateProcessW`. Because op-reason's own command line
already holds op's arguments, this only triggers when the `op.exe` path is much longer than the exe's. (A' carries
the arguments in an environment variable as base64 and refuses above 28,000 characters of base64.)

## 9. Closing the CLI session

Same as the launcher: after `op.exe` exits, run `op.exe whoami` with no window and all streams redirected; if it
succeeds, run `op.exe signout --all`. The `end` record's `signout` is `ok`, `not-signed-in`, `failed: ...`, or
`skipped` (service mode).

`signout --all` also closes approvals of other processes (an `op` typed by hand in another tab, or a WSL call that
the WSL watchdog runs outside the mutex). The mutex keeps op-reason calls from overlapping. Whether a plain
`signout` closes only this process's approval is not verified yet (a check by hand on the author's machine).

## 10. Interruption

- Ctrl+C and Ctrl+Break reach every process on the console, including `op.exe`. The exe's console handler only
  sets a flag. The main loop gives `op.exe` two seconds to exit on its own, terminates it otherwise, closes the
  session (§9), writes `end` with `result: interrupted`, `exit: 130`, and exits with 130. While waiting for the
  lock, it writes `refused` with `why: interrupted` instead.
- Unlike WSL (where an interrupted call leaves the approval on a dead `powershell.exe` until it times out), the
  exe is still alive after Ctrl+C and can sign out.
- Closing the console window gives a handler about five seconds: the exe writes `end` (`interrupted`, 129) and
  lets Windows end the process. A forced kill (the Claude Code tool timeout kills the process tree) writes nothing,
  like SIGKILL on WSL.

## 11. Records and text

- The same `v: 1` lines as WSL, written under `log.lock` and rotated at 1 MiB. Times are local with offset and
  epoch seconds. `watch` is `{start: null, end: null}` (no watchdog on Windows).
- The core's messages (usage, refusals, lock) are shared with the WSL binary. The window and toast texts live in
  `launcher.ps1` in A'; C' holds a Rust copy of that table, and a test reads the table out of `launcher.ps1` and
  compares, so that the two windows cannot drift apart silently.
- `op-reason --log` (the core's log viewer) works on Windows too, reading `%LOCALAPPDATA%\op-reason\`. A' has no log
  viewer on Windows (A' spec §7).

## 12. Calling the exe from WSL too (discussion only)

Shape: the WSL binary would start `op-reason.exe` with the facts (the A' payload) instead of
`powershell.exe -EncodedCommand ...` with `launcher.ps1`.

Benefits:

- One window implementation (§7.4 caveat goes away) and one place for stage ④.
- No Windows PowerShell start per WSL call (the measured start of the exe is tens of milliseconds; a
  `powershell.exe` with WPF takes hundreds).
- No launcher script transported through an environment variable to stay under the 32,767-character limit.
- Typed, unit-tested Rust instead of PowerShell 5.1 for the launcher logic.

Risks:

- Smart App Control: if the unsigned exe is blocked, WSL calls stop working too. Today WSL only needs the signed
  `powershell.exe` that ships with Windows. Every new version starts again without reputation unless signed.
- Delivery: a Linux binary would have to place a Windows exe on a Windows path (for example under
  `%LOCALAPPDATA%\op-reason\`), keeping the two in version step. Running it from `\\wsl.localhost\...` may be
  treated as a network location by SmartScreen and SAC.
- Antivirus heuristics may dislike an exe written to disk by a Linux process and then started.
- One more failure mode in the WSL path that is hard to see from Linux.

Middle ground, if ever wanted: keep `launcher.ps1` as the WSL default, and let WSL use the exe only when the user
opts in and the exe is signed; fall back to the launcher when the exe fails to start.

## 13. Signing and Smart App Control

The author's machine has Smart App Control on. SAC lets an unsigned exe run only after the cloud has built
reputation for that exact file (hash). In the author's other Rust tool, some new builds ran at once and others
(including v0.2.2) were blocked; each new version starts from zero. A signed exe is judged by its certificate;
Microsoft's guidance is that SAC lets through binaries signed with a certificate from a trusted CA (not verified
for this project, and there are reports of signed binaries being blocked).

SignPath Foundation signs open-source projects for free. Conditions read on 2026-11-15 (signpath.org/terms):

- An OSI-approved license, not dual-licensed with a commercial license; no proprietary closed-source code; the
  project is maintained.
- "Released: The project must already be released in the form that should be signed."
- Everyone involved uses multi-factor authentication on SignPath and GitHub.
- The repository or the project site names the roles (Authors, Reviewers, Approvers) and says
  "Free code signing provided by SignPath.io, certificate by SignPath Foundation".
- Artifacts are built in a way that can be verified from the source; every build of a version has the same
  version attributes.
- No features that circumvent the security of the runtime environment.

What op-reason would need before applying:

1. Ship `op-reason.exe` unsigned in a GitHub Release (for example next to the PowerShell zip, marked as a
   preview), because the signed form has to be released first. SAC users will see it blocked until signed.
2. Turn on MFA for every maintainer on GitHub and SignPath.
3. Add the roles and the sentence above to the README (not in this PR).
4. Build the release exe only in GitHub Actions from a tag, with the version resource and the manifest version
   taken from `Cargo.toml`, and submit it to SignPath from the same workflow (SignPath's GitHub integration checks
   that the artifact came from that repository, workflow and tag). The manifest already carries the version; a
   version resource (`VS_VERSIONINFO`) is still to add.
5. Keep the license MIT OR Apache-2.0 and keep the code free of anything that disables or works around Windows
   security features.

Until the exe is signed, the PowerShell entry stays the safe default on SAC machines, which is why A' is the
default plan.

## 14. Build, layout and tests

- `crates/op-reason-win` builds `op-reason.exe` for `x86_64-pc-windows-msvc`. It is a Cargo workspace of its own
  (with the test helper `crates/op-reason-win/fake-op`) and uses `op-reason-core` by path: the `windows` crate is a
  9 MB download and 114 MB unpacked, and in the main `Cargo.lock` the Nix package of the WSL binary would fetch it for
  nothing (Nix vendors every crate of the lock file, whatever the target). Cargo runs from that directory, where
  `rust-toolchain.toml` pins the toolchain of the Windows CI job.
- Its own code:
  - `src/win/` (Windows only): the flow, the lock and its holder file, the process list, the window, the toast,
    starting op.exe and signing out, console events.
  - `src/ui.rs`: the launcher's `Build-Ui` and its text table in Rust. A test reads the table out of
    `launcher/launcher.ps1` and fails when a text differs.
  - `src/cmdline.rs` (the command-line length) and `src/messages.rs` (the three messages whose core wording speaks
    of WSL or of the payload).
  - Everything else (planning, masking, place, records and their file, result, messages, log viewer, time) is the
    core's, the same code the WSL binary runs.
- Dependencies of the exe: the core, `serde`/`serde_json` (the holder file), `windows` and `windows-numerics`
  (Microsoft); build dependency `embed-manifest` (no `rc.exe`). Versions are at least 7 days old when locked; the
  core's dependencies are locked at the same versions as in the main lock file.
- Release profile: LTO, one codegen unit, stripped.
- Tests:
  - The core's unit tests and the call table cover the planning that the exe uses as is.
  - On Linux: the window texts against `launcher.ps1`, the command-line length.
  - Integration tests on the Windows runner with a fake `op.exe`: tricky arguments arrive one by one unchanged
    (empty, spaces, quotes, trailing backslashes, `&|<>^%!`, Japanese, newline, tab, emoji, an unpaired surrogate),
    stdin bytes pass through, stderr carries only op's text, the exit code is op's, the environment per kind, `whoami`
    then `signout --all`, refusals and their records, `denied` and `timeout`, two concurrent calls wait for the mutex
    and the second times out with 75 naming the holder, the window really shows, the wrapper options, the log viewer.
  - A screenshot job that shows the window with a sleeping fake `op.exe` and captures it, next to the A' launcher's
    window with the same facts, and a timing job that measures (black box, from process start) how long until the
    window is visible and how long a whole call takes, for the exe and for the A' launcher started directly. The
    screenshots go to the pull request as one comment.
- Local checks from Linux: `cargo clippy --target x86_64-pc-windows-msvc` (needs only the target's standard
  library). The exe is not started on the author's machine by the agent.

## 15. Differences from the A' spec (for review)

| Point | A' (Windows) | C' | Why |
| --- | --- | --- | --- |
| Lock time-out record | `start`, then `end` with `lock-timeout` (the launcher waits after `start`) | `refused` with `lock-timeout` (the exe waits before `start`, like the WSL flock) | no `start` for a call that never ran op |
| Interrupted | not defined for the Windows entry | 130 (Ctrl+C, Ctrl+Break), 129 (console closed) | the WSL numbers |
| Unusable mutex | not defined | run without it, `start.lock` = `failed` | do not block the user |
| Too long | base64 of the arguments over 28,000, or a payload over 32,000 | the `op.exe` command line over 32,766 | no payload |
| Caller | the process chain from CIM | the same rules; the chain from a Toolhelp snapshot, which also guards against reused PIDs | faster, and a reused PID does not join the chain |
| Log viewer | none on Windows | `op-reason --log` | the core's viewer is Rust |
| Messages | the PowerShell entry's own texts | the core's texts, which say `op` (the WSL command name) in the usage; Windows wording for a missing `op.exe` and for the length limit | one copy of the texts |

## 16. Comparison with A'

Filled in the PR description with numbers from CI: lines written, start time, what the window can do, SAC and
signing, `cmd.exe` re-reading the arguments, distribution, and upkeep.
