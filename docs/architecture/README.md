# How op-reason works

This page describes op-reason as it is now. Why it is built this way is in the design documents
([docs/superpowers/specs/2026-11-15-op-reason-design.md](../superpowers/specs/2026-11-15-op-reason-design.md),
[2026-11-15-windows-exe-design.md](../superpowers/specs/2026-11-15-windows-exe-design.md),
[2026-11-15-wsl-through-exe-design.md](../superpowers/specs/2026-11-15-wsl-through-exe-design.md) and, for the SSH relay,
[2026-11-17-ssh-relay-design.md](../superpowers/specs/2026-11-17-ssh-relay-design.md)).

![op-reason components](plantuml/out/01_overview.svg)

## Parts

| Part | Where | Role |
| --- | --- | --- |
| WSL executable | `crates/op-reason/` | `op` / `op-reason`: the wrapper; `op-reason-log`: the log viewer; `op-reason-ssh`: runs `ssh.exe` with the facts of the call for the SSH relay. Linux only (`/proc`, flock, signals, `WSLENV`). Starts a runner for every call |
| Core library | `crates/op-reason-core/` | argument planning, the runner contract, log records and the log viewer, place detection, messages, and the SSH relay's portable parts (`ssh`: the agent protocol, Windows command lines, the roots of calls, `OPREASON_SSH_CALL`, the decision book and the queue, the SSH records); builds for Windows too |
| Windows executable | `crates/op-reason-win/` | `op-reason.exe`, three faces: the Windows entry (the whole call in one process: Direct2D window, WinRT toast, `op.exe`, sign-out, log, `--log`), the runner of calls from WSL (`--run`) and the SSH relay (`--ssh-agent`). A Cargo workspace of its own, on the core |
| Launcher | `launcher/launcher.ps1` | Windows PowerShell 5.1 script, the fallback runner: started fresh for a call from WSL when `op-reason.exe` cannot run, and for every call through the PowerShell entry: window, toast, `op.exe`, sign-out, status |
| PowerShell entry | `windows/` | `op-reason.ps1` and `op-reason.cmd` (a shim for cmd.exe and other programs): the fallback where Smart App Control blocks `op-reason.exe` |
| Fish completion | `completions/op.fish` | template printed by `op-reason --completion fish` |
| Skill | `skill/SKILL.md` | printed by `op-reason --skill` |
| Call table | `tests/cases/calls.json` | invocations and their expected plan, shared by the Rust tests and the Windows entry's tests |
| Root table | `tests/cases/roots.json` | Windows and `/proc` process chains and the root of their call, run against the core's rules (the relay and `op-reason-ssh` use them) |

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
5. **Start** record; the window (Direct2D and DirectWrite, the launcher's texts drawn as a view: the reason first, chips,
   the command's box, the band; dark or light as Windows' app mode; at the top right of the work area of
   the screen of the foreground window, with that screen's DPI) and the toast (on a worker thread; tagged, and removed
   from the screen and the notification center when the window closes, unless no window could be shown);
   `op.exe` with the caller's stdin and stdout, its stderr copied through while the tail is kept for `denied` and
   `timeout`. A desktop `op inject` without `-i` reads its template from stdin before the lock (when stdin is not a
   console), and `op.exe` gets the same bytes. `op.exe` runs in a kill-on-close Job Object (created suspended, put in
   the job, resumed): if the exe dies, `op.exe` ends too; when `op.exe` ends by itself the flag is cleared.
6. **Close** the session (`whoami --format json`, then `signout --all` when signed in; not for service calls; the
   account ID and host of a whoami that succeeded go to `account.json`), **end** record, remove `holder.json`, release
   the mutex and exit with `op`'s code. Ctrl+C: `interrupted`, 130; the console closed: 129.

The window and toast texts are a Rust copy of the launcher's table (`crates/op-reason-win/src/ui.rs`). A test reads
`Get-Texts` out of `launcher.ps1` and fails when the two differ: change both together. The window of `op-reason.exe`
draws a view built next to those lines (`look.rs`: the parts top to bottom, the colored words of the command, the
switch's pill), with a few words of its own (`Words` in `ui.rs`: the subtitle, the band), in the palette of Windows' app
mode (`theme.rs`; `win/theme.rs` reads `AppsUseLightTheme` and sets the frame: no caption text, the caption in the
background color, dark in the dark mode, rounded corners;
[2026-11-26-modern-window-design.md](../superpowers/specs/2026-11-26-modern-window-design.md)). The launcher's WPF
window keeps its plain look.

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

## The SSH relay (op-reason.exe --ssh-agent)

A resident process in front of the 1Password SSH agent
([2026-11-17-ssh-relay-design.md](../superpowers/specs/2026-11-17-ssh-relay-design.md)). It serves the agent protocol on
its own pipe (default `\\.\pipe\op-reason-ssh-agent`: created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, a DACL for the
current user only, `PIPE_REJECT_REMOTE_CLIENTS`, unlimited instances) and passes every message on unchanged to the
1Password agent (`--upstream`, default `\\.\pipe\openssh-ssh-agent`), over one upstream connection per client
connection, opened at the first message. Messages are framed by a 32-bit length up to 256 KiB (a longer one closes the
connection). One relay per logon session (the mutex `Local\op-reason-ssh-relay`); `--status` connects to the pipe and
names the server's process.

Threads: the main thread accepts connections; one thread per connection reads its requests in turn; the thread whose
turn it is shows the window; one forgets decisions once a minute; one watches the relay's parent process (when it has
ended, 1Password most likely takes the relay itself for the application that asks; a toast and a `ssh-relay` record say
so); one waits for Ctrl+C while a console is attached (a console that Windows made for the relay alone, on Windows
before 11 24H2, it lets go of with `FreeConsole`; its `start` record names the console it had: `console: none`, `own` or
`shared`); one
looks once a second at the step a window's thread waits in (making the window, showing it, drawing it, `holder.json`,
the notification: process-wide atomics of `src/win/window.rs`) and records `window-stuck` with the step when one has
lasted five seconds, and `window-unstuck` when it ends (`src/relay_watch.rs` decides); and, from the first signature
that passes without a window, one owns the toasts' window
([2026-11-18-pass-toasts-design.md](../superpowers/specs/2026-11-18-pass-toasts-design.md)). At start the relay also
reads its ancestors (PID, creation time, name), to tell whether the window in front belongs to the program that
started it. The relay's windows draw into their device context with Direct2D's software renderer (a DC render target:
no GPU, no swap chain, no wait for the display's refresh); the windows of `op` calls keep Direct2D's default
([2026-11-22-front-after-allow-design.md](../superpowers/specs/2026-11-22-front-after-allow-design.md) §12).

### One request

1. **Read** the message. `session-bind@openssh.com` is read (the host key's fingerprint; with the forwarding flag set,
   the connection carries forwarded requests: the first such bind is the remote host, a later bind without the flag the
   remote client's destination). The answers to the key list are read for the keys' comments. Anything but a sign
   request is passed on; when 1Password takes more than a second, the window without buttons comes (1Password may be
   asking to unlock).
2. **Who asks** (once per connection): the client's PID (`GetNamedPipeClientProcessId`), its chain from one Toolhelp
   snapshot (a parent created after its child ends the chain), each process's command line
   (`ProcessCommandLineInformation`), and the client's environment and directory (from its PEB). The root by the rules
   of `tests/cases/roots.json`: a `claude.exe` ancestor's child; else an interactive shell's child (a shell whose
   arguments run no command and no script; this rule also gives the shell, for the terminal answer); else the client's
   parent. A client whose parent is `wsl.exe`, `wslhost.exe` or `C:\Windows\System32\bash.exe` is a WSL call: its root,
   caller, place and reason come from `OPREASON_SSH_CALL` in its environment, or it has none (each signature a call of
   its own). Windows clients give their reason in `OPREASON_SSH_REASON`. A root or shell whose start time cannot be read
   is not remembered. The terminal answer is offered only for a root among `ssh`, `scp`, `sftp`, `rsync`, `git`,
   `ssh-keygen`, `ssh-add`, `sshfs`, `mosh`, `ansible*` and `op-reason-ssh`, under a shell whose command line could be
   read. No 1Password agent: the request fails at once (`no-upstream`).
3. **per-call**: the mark is the root (scope, PID, start time), the key's fingerprint and the reason (a forwarded
   request, one without a root, and one from an `ssh.exe` connection that signs before any `session-bind@openssh.com`
   (OpenSSH before 8.9: rule `unbound`) are each a call of their own). A request of a call whose request is in its
   window or with 1Password follows it; an answer in the book (or, for a person's command, the terminal's) passes or
   fails the request at once. One that passes and that 1Password signs (without the window without buttons on the way)
   gets a toast at the top left of the screen of the foreground window: a layered, click-through window that never takes
   the focus, one card per call (×N, the last destination) at 90% opacity, three at most and `+N` under them for the
   cards pushed out, each gone four seconds after its call's last signature (`crates/op-reason-win/src/pass_toast.rs`
   decides the cards, `src/win/pass_window.rs` draws them, each card in a Direct2D layer at its opacity). Otherwise the
   request joins the queue, first come first asked. On its turn the thread takes `Local\op-reason` (up to
   `OPREASON_LOCK_WAIT` seconds; `lock-timeout`), writes `holder.json`, shows the window with its buttons, the bar of
   the seconds left to answer and the toast, and keeps the count of waiting calls current. The window's box shows the
   root's command, with the wrappers of a shell and of Claude Code's Bash tool taken off when they match exactly (from
   the record's line of up to 1000 characters; `look::root_command`), or, for the launcher of Claude Code's PowerShell
   tool, which carries no command, the ssh command (told in `win/relay/conn.rs` from the root's own arguments, for a
   call of Claude Code: `look::claude_powershell`), and says how long an answer lasts with its name ("this call" for the
   PowerShell tool). The toast (the Windows notification) is removed when the window closes, except after a time-out
   (`timeout`), which leaves it. A click (press and release on a button) is remembered for the call (Allow this
   terminal: also for the shell and the key); after Allow the window drops its buttons ("1Password is asking", with a
   band that says where 1Password keeps a prompt it did not show) until 1Password answers, then closes, and the lock and
   the turn pass on. Right after a click that allows, the window comes to the front (`SetForegroundWindow`: the click
   was the last input, so Windows lets the relay set the foreground window), unless the window in front belongs to one
   of the relay's ancestors other than `explorer.exe` (the terminal program a relay started in one runs under, which
   1Password takes for the application); then the relay lets the process that serves 1Password's pipe
   (`GetNamedPipeServerProcessId`) set the foreground window (`AllowSetForegroundWindow`), and sends the request. From
   that moment the window watches for 1Password's prompt every 100 ms (`win/prompt.rs` looks at the window in front and
   the windows of 1Password's processes that a person can see, `relay_prompt.rs` decides;
   [2026-11-26-prompt-guide-design.md](../superpowers/specs/2026-11-26-prompt-guide-design.md)): a window of 1Password
   (the pipe's server or `1Password.exe`) or of Windows Hello in front, or a new window of 1Password, is the prompt
   ("Approve in 1Password's window"); none within two seconds lights the window up (a border and, on Windows 11, the
   frame's border in the caution color), counts down about 60 seconds, and shows a click-through pointer next to the
   notification area at the 1Password icon (`win/tray_hint.rs`), until the prompt comes or the request ends. When the
   window closes, it gives the front back to the window that had it, if it still has the front or no window has it
   ([2026-11-22-front-after-allow-design.md](../superpowers/specs/2026-11-22-front-after-allow-design.md);
   `src/relay_front.rs` decides, `src/win/window.rs` comes forward and gives back). No click within `--answer-wait`
   (`timeout`) and the close button (`dismissed`) fail the request and the requests that followed it, without
   remembering; a client gone with no request of its call waiting (`gone`) closes the window. Buttons take a press only
   400 ms after they appear, and the window captures the mouse from a press on a button until its release.
4. **show-only**: every signature waits for its turn and the lock, shows the window without buttons, goes to 1Password
   and closes the window with the answer. A signature that passes in under half a second gets a toast once per run, and
   a `no-prompt` record (1Password most likely did not ask).
5. **Answer** the client with 1Password's reply (or `SSH_AGENT_FAILURE`), and **record** the signature. A request that
   1Password closes without an answer, or does not answer within 300 seconds, is `no-answer` (a toast once per run says
   where 1Password keeps a prompt it did not show). A connection to 1Password that has closed since is opened again
   once before a request is sent; it identifies the relay's user to 1Password's pipe but does not let the server act as
   it (`SECURITY_IDENTIFICATION`). Four instances of the relay's pipe wait for clients at once.

Decisions live in memory only: forgotten when a Windows root ends (checked on lookup and once a minute), after the time
limit (`--time-limit`, default 60 minutes; WSL roots only time out), or when the relay exits.

### Records

`%LOCALAPPDATA%\op-reason\ssh-log.jsonl`, one JSON object per line, rotated at 4 MiB (`ssh-log.1.jsonl`,
`ssh-log.2.jsonl`), appends taking `ssh-log.lock`. Never the data to sign or a signature. `op-reason.exe --log` merges
them with `log.jsonl` by time, one line each; a request that an earlier answer covered shows the caller and reason of
the record with that answer.

| `event` | Fields |
| --- | --- |
| `ssh-sign` | `v`, `id`, `time`, `epoch`, `mode` (`per-call`, `show-only`), `result` (`allowed`, `passed`, `denied`, `timeout`, `dismissed`, `gone`, `signed`, `failed` (1Password refused), `no-answer`, `no-upstream`, `lock-timeout`, `no-window`, `malformed`), `seconds` (to a tenth), `answer` (`allow`, `allow-terminal`, `deny`), `decision` (the `id` of the record whose answer covered it; then the call's facts are there), `call` (`rule`: `claude`, `shell`, `parent`, `alone`, `unbound`; `root` and `shell`: `pid`, `start`, `name`, `cmd`; `distro`), `caller` (as in `start`; `type` also `program`), `session`, `reason` (≤ 500), `client` (`pid`, `exe`, `cmd` masked, ≤ 1000), `key` (`fp`, `comment`, `type`), `purpose` (`kind`: `userauth` with `user`, `sshsig` with `namespace`, `other`), `host` (`name` from the command line, `fp`, and `onward` for a forwarded connection), `forwarded`, `prompt` (after a click that allows: `front`, whether the window came to the front; `seen`, the seconds to 1Password's prompt or null; `hint`, the seconds to the guide (the window lit up, and the pointer at the tray) or null) |
| `ssh-relay` | `v`, `id`, `time`, `epoch`, `what` (`start`, `exit`, `no-prompt`, `no-parent`, `parent-ended`, `window-stuck`, `window-unstuck`), `mode`, `pipe`, `upstream`, `limit` (seconds), `pid`, `parent`, `detail` (`start`: the console it had, `console: none`, `console: own` or `console: shared`; `window-stuck`: the step its thread waits in, one of `factories`, `layout`, `create`, `show`, `target`, `draw`, `holder`, `toast`; `window-unstuck`: `after <seconds> s`) |

Command lines are masked with the rules of `op` calls, word by word inside a remote command, without the rule for what
follows `--`.

### op-reason-ssh (WSL)

The WSL executable under the name `op-reason-ssh` walks `/proc` from itself (stat and cmdline; a parent that started
after its child ends the chain), applies the same rules to find the root (`claude`, an interactive shell, the parent),
collects the caller as the wrapper does (Claude Code, `CLAUDE_CODE_SESSION_ID`, herdr's workspace with its label within
a second, the place), `WSL_DISTRO_NAME` and `OPREASON_SSH_REASON` (normalized), and puts them into `OPREASON_SSH_CALL`
(ASCII JSON, at most 8000 characters: command lines, reason and directory are cut shorter until it fits) with
`OPREASON_SSH_CALL` added to `WSLENV`. Then it `exec`s `ssh.exe` (`OPREASON_SSH_EXE`, else `ssh.exe` in `PATH`) with
the arguments and streams unchanged. When `/proc` cannot be read it runs `ssh.exe` without the variable; no `ssh.exe`:
127.

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

## Settings

`op-reason --settings` ([2026-11-23-settings-design.md](../superpowers/specs/2026-11-23-settings-design.md)) reads and
changes op-reason's settings through one layer:

- `op_reason_core::settings` holds the list of settings, their values, `settings.json` (edited with other fields kept),
  op-reason's task (its XML and the shape a task read back must have), the skill's mark, local places and the shell
  script for WSL distributions, the command line's words, texts and JSON, the `settings` record, and the decisions
  (`engine`: `list`, `get`, `check`, `set`, the relay's words) over a `System` trait. Tests run the engine on a fake
  system.
- `op-reason.exe` implements `System` (`crates/op-reason-win/src/win/settings/`): Task Scheduler's COM API for
  `\op-reason-ssh-relay-<SID>`, the relay's pipe, `relay.json` and its stop event `Local\op-reason-ssh-relay-stop-<pid>`,
  op-reason's lock `Local\op-reason` for stops and restarts, `wsl.exe` for the distributions, and `Local\op-reason-settings`
  around writes of `settings.json`.
- The relay reads `relay.*` of `settings.json` before its options, writes `relay.json` at start, and on its stop event
  fails new requests, waits up to 25 seconds for the ones in progress, records `stop` and exits.
- The WSL executable passes `--settings` on to `op-reason.exe --settings`; the PowerShell entry refuses it.
- Without more words (and on a double click of `op-reason.exe`), `op-reason.exe --settings` starts
  `op-reason.exe --settings window` without a console window (`CREATE_NO_WINDOW`; `wsl.exe` is started with
  `CREATE_NO_WINDOW` too) and returns. That process holds `Local\op-reason-settings-window`
  and shows the window of class `op-reason-settings` (`win/settings/window.rs`, its standard controls in `controls.rs`):
  a worker thread runs the engine for every control (`from: window`) and reads every setting again after each job; its
  texts and lines are portable (`settings_ui.rs`). The mode and the times go through `engine::write`, which leaves the
  relay alone; Apply restarts it.
- The relay shows an icon in the notification area (`win/relay/tray.rs`; its texts, menu and drawn key in
  `tray_ui.rs`): a hidden window of class `op-reason-ssh-relay-tray` that writers of `settings.json` tell with the
  registered message `op-reason-settings-changed`; its menu runs `op-reason.exe --settings …` with
  `OPREASON_SETTINGS_FROM=tray` (`from: tray`), for the relay of op-reason's task with Explorer as the parent process
  (out of Task Scheduler's job). `start-menu` makes `op-reason.lnk` through `IShellLinkW`.

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
call holds the mutex, or the SSH relay's window is open), `settings.json` (the language chosen in the window, the relay's
values and the chosen skill folders), `relay.json` (the running relay's process ID and values),
`account.json` (the account IDs and hosts that link references to the 1Password app), the SSH relay's records
(`ssh-log.jsonl`, `ssh-log.1.jsonl`, `ssh-log.2.jsonl`, `ssh-log.lock`) and, for calls through the PowerShell entry,
`run/<id>/status.json`.

## Consoles

`op-reason.exe` is a console program, so that terminals wait for it and give it their console. Its manifest asks Windows
for no console that it does not inherit (`consoleAllocationPolicy` `detached`;
[2026-11-24-relay-no-console-window-design.md](../superpowers/specs/2026-11-24-relay-no-console-window-design.md)):
on Windows 11 24H2 and later and Windows Server 2025, a start without a console to inherit (a scheduled task, a
program without a console that passes no flags, Explorer's `CREATE_NEW_CONSOLE`) has no console at all, where Windows
before made one and showed it. `CREATE_NO_WINDOW` (WSL's start of a call without a terminal) still gets a console that
is never shown. Older Windows ignore the element. So:

- a terminal's console, WSL's pseudoconsole and the console of `CREATE_NO_WINDOW` are as before (`op.exe` shares
  them);
- without a console, `op.exe` starts with `CREATE_NO_WINDOW` (`win/opexe.rs`), and so do `whoami`, `signout` and
  `wsl.exe` always;
- without a console and without a standard output that goes anywhere (none, or the monitor that a start from the
  taskbar puts there), `op-reason.exe` without arguments is a double click: it opens the settings window
  (`win/console.rs`, `started_alone`);
- the relay of a task has no console from its start; on older Windows it lets go of the one Windows made
  (`FreeConsole`).

## Tests

| Test | What it covers |
| --- | --- |
| `cargo test --workspace` | the core (including the call table, the reference table and the root table; the agent protocol, command lines, `OPREASON_SSH_CALL`, the decision book and the queue, the SSH records and their lines in the viewer) and the WSL helpers (the `/proc` chain) |
| `tests/wrapper.test.sh` | the executable with fake `op-reason.exe`, `powershell.exe`, `op.exe`, `wslpath` and `herdr`, including the runner choice and every fallback case (`op.exe` counted: never twice) |
| `tests/launcher.test.sh` | the launcher on PowerShell 7 for Linux with a fake `op` |
| `tests/e2e.test.sh` | the executable and the real launcher together (`-File`), and a fallback from an exe that writes nothing |
| `tests/log.test.sh` | the log viewer |
| `tests/ssh.test.sh` | `op-reason-ssh` with a fake `ssh.exe`, a fake `herdr` and a fake `/proc`: arguments, stdin, stderr and the exit code unchanged, `WSLENV`, `OPREASON_SSH_CALL` (rule, root, shell, caller, herdr, place, distribution, reason), a `/proc` it cannot read, `OPREASON_SSH_EXE`, no `ssh.exe` |
| `tests/completion.test.sh` | the fish completion with the real fish |
| `tests/settings.test.sh` | `op-reason --settings` on WSL with a fake `op-reason.exe` and `wslpath`: arguments, exit code, the caller in `WSLENV`, folders as Windows paths, the copy of the language |
| `tests/windows/entry-plan.test.ps1` | the call table and the reference table against the Windows entry (Windows PowerShell 5.1 in CI, PowerShell 7 on Linux) |
| `tests/windows/launcher.test.ps1` | the launcher on Windows PowerShell 5.1 with a fake `op.exe` (arguments, binary stdin, the mutex, stderr scan, the references and the account copy, the window and its screenshots; on CI, a real click on a link with a temporary `onepassword:` handler) |
| `tests/windows/entry.test.ps1` | `op-reason.ps1` and `op-reason.cmd` end to end on Windows PowerShell 5.1 |
| `cargo test` in `crates/op-reason-win` (Linux) | the exe's window and toast texts against `launcher.ps1` (the block of references too), the view of a call's window, the palettes (every text color at 4.5:1 on its background in both modes), the colored words of a command, the rows of chips and the switch's pill, its usage, the payload and status of `--run`, the command-line length, the account copy and the item URL |
| `cargo test` in `crates/op-reason-win` (Linux): the relay | its options, the view of its window (both phases, both languages, the root command with the wrappers taken off, how long an answer lasts, the band while 1Password asks) and toasts, its usage, the toasts of signatures that pass (their texts, one card per call, the fourth card and `+N`, the opacity, the times, the counts, no motion), when the window stays behind (the program that started the relay in front), when the watchdog records a window stuck and going on, the watch for 1Password's prompt after Allow (seen, the pointer after two seconds, a prompt that comes later; a menu or a tooltip never counts), the taskbar's edge from its rectangle and where the pointer goes for a taskbar at each edge, the texts of the window after Allow and of the pointer |
| `crates/op-reason-win/tests/relay.rs` (Windows) | `op-reason.exe --ssh-agent` with the fake agent, client and `wsl.exe` of `fake-ssh`: messages unchanged, a signature waiting for the window, an unanswered one not remembered, Allow and Deny (real clicks on CI) covering the call, two requests of a call and one answer, the time limit, no agent, the pipe's DACL and a name taken first, one relay per session, `--status`, op-reason's lock, a WSL call with and without `OPREASON_SSH_CALL`, show-only and its toast, a forwarded connection, a slow answer, no answer, a parent that ends; on CI, the toast of signatures that pass (one card, layered, click-through, at the top left of the work area, gone after four seconds; four calls: three cards and `+1`), the window's notification in the notification center (removed with its window, left after a time-out; nothing is checked where the runner keeps no notification history), the icon in the notification area (added, hidden and shown again at once by `relay.tray-icon`, Hide this icon recorded `from: tray`, none when the setting is off), and the front (with a plain window of `fake-ssh front` in front: the window opens behind, Deny leaves the front, after Allow the window is in front while the agent answers and then gives the front back; with that window's process as the relay's parent, the window stays behind; no record of the watchdog of the windows), and after Allow the pointer to the tray (it comes after two seconds without a prompt, click-through and never active, and goes with the request; with a window of the fake agent in front, standing for 1Password's prompt, none comes; the records' `prompt`) |
| `crates/op-reason-win/tests/settings.rs` (Windows) | `op-reason.exe --settings`: the skill on Windows and in a chosen folder with a temporary profile (a foreign `SKILL.md` and a junction refused), a WSL distribution through the fake `wsl.exe` of `fake-ssh`, the Start menu shortcut in a temporary `APPDATA`; on CI, the settings window (`op-reason --settings` returns at once and one window opens, a double click opens it, its controls place the skill and write `lang`, no console window for the programs it starts), op-reason's task under a test name (made, its relay answering, a stop refused while the lock is held, a stop that outlasts the trigger of every minute, a start, a mode change that starts the relay again, removal; no console window and a relay without a console at the task's start, a restart and the icon's restart) and a relay started outside the task, never stopped |
| `crates/op-reason-win/tests/exe.rs` (Windows) | `op-reason.exe` end to end with a fake `op.exe`: arguments, stdin, stderr, exit codes, the mutex and the holder, refusals and records, the window, `--` from PowerShell, the log viewer; `--run` in every mode (the first write, the lock time-out, `lock: skipped`, bad payloads, an unwritable status file); killing the exe ends `op.exe` (Job Object); the manifest's console allocation policy, a call started as WSL starts one without a terminal (`CREATE_NO_WINDOW` and pipes) and as a program without a console starts one (`DETACHED_PROCESS` and pipes): no console window for `op.exe`; a call under a pseudoconsole (as from a terminal); the chosen language in both faces and `chosenLang`; the references (`ui.refs`, a template on stdin, files), the account copy from whoami; a desktop call's notification removed when its window closes (where the runner keeps notification history); on CI, a real click on the language switch (the caption switches, `settings.json` is written, the focus stays) and on a link (a temporary `onepassword:` handler gets the item's URL, the window stays inactive) |
| `.github/scripts/windows-exe-shots.ps1` | screenshots and timings of `op-reason.exe` (both faces) and the PowerShell entry on the Windows runner, and the SSH relay's window (per-call and show-only; after Allow, lit with the pointer to the tray and the whole screen, and with a stand-in for 1Password's prompt in front) and toasts, in Windows' dark app mode and, for a call's window with references, the relay's windows (the lit one after Allow too) and the toasts, in the light mode too |
| `.github/scripts/windows-dist.ps1` | the Windows zip of the release and its entries |
| `tests/release-notes.test.sh` | `.github/scripts/release-notes.sh` (the notes of a release: the version's section of `CHANGELOG.md`; the versions of `Cargo.toml`, `crates/op-reason-win` and the PowerShell entry agree, also with the tag) on made-up repositories and on this one |
| [manual-verification.md](../manual-verification.md) | checks with the real 1Password app (by hand) |
