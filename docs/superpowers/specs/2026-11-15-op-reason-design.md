# op-reason design

- Date: 2026-11-15
- Status: approved (the design was agreed before this document was written; this document records it and the
  implementer's decisions)
- Scope of the first two pull requests: PR 1 (WSL side in Rust, CI, docs) and PR 2 (Windows entry, launcher lock,
  Windows CI)

## 1. What and why

op-reason is a wrapper for the 1Password CLI (`op`). It:

1. requires a reason for every call (`--reason "<why>"`),
2. confines the approval of the 1Password desktop app to that one call,
3. shows a window with the caller and the reason while the 1Password approval prompt is open, and
4. records every call (start, end, refusal, shared approval found) as JSON Lines.

It started as a personal tool in the author's dotfiles: a bash script for WSL (about 860 lines), a PowerShell
launcher (about 310 lines), a log viewer, a fish completion and black-box tests. It moves to its own public repository
so that GitHub Actions (including the Windows runners) are free to use: the approval window is going to get richer, and
a public repository can take screenshots of the window in CI and show them on pull requests.

### 1.1 Why a wrapper is needed

- **Approval binding.** The 1Password desktop app binds a CLI approval to the process that started `op.exe`. From WSL,
  every `op.exe` is a child of the same `wslhost`/`wsl.exe` that all terminal panes share, so one approval reaches later
  calls and other panes. On Windows, starting `op.exe` twice from the same process also reuses the approval. op-reason
  starts a fresh `powershell.exe` (the launcher) for every call, starts `op.exe` from it, and signs out at the end, so an
  approval lives for one call only.
- **Who is asking.** The 1Password prompt names only the terminal (`WindowsTerminal`, or `wslhost` from WSL), whatever
  process asked. The op-reason window shows the caller (a person or an AI agent such as Claude Code), where it runs (repo,
  branch, directory) and the reason.
- **Audit trail.** Every call is recorded, so calls made by AI agents can be reviewed later.

### 1.2 Facts this design relies on (checked by hand on 2026-11-14, Windows 11, Smart App Control on)

- An approval is bound to the process that started `op.exe` and does not reach other processes. Two `op.exe` started by
  the same parent showed one prompt; separate tool calls, or a fresh child `powershell.exe` per call, showed one prompt
  each; two processes calling at the same time showed two prompts.
- The existing launcher ran unchanged on Windows (started from Windows, not WSL): window at the top right, Japanese text
  rendered, toast shown, one prompt, exit code 0, status
  `{"started":true,"exit":0,"error":"","window":"shown","toast":"shown","others":0,"signout":"ok"}`.
- The 1Password prompt said "WindowsTerminal" whichever process asked, so the reason window is needed on Windows too.
- `signout --all` closed the approvals of other processes too (siblings, children, and an `op.exe` typed by hand in
  another terminal tab). `whoami` did not affect other processes.
- Tools seen on the author's Windows: Claude Code's tool shell is pwsh 7.7 and every tool call starts
  `claude.exe → cmd.exe → pwsh.exe` afresh; Windows PowerShell is 5.1.26100; `git.exe` is not installed; the execution
  policy is `Bypass` for the process and `Undefined` (Restricted) elsewhere.
- An unsigned local `.ps1` ran in FullLanguage with `-ExecutionPolicy Bypass` (WPF and `Add-Type` worked), also with the
  Mark of the Web and through a `.cmd` shim. Without `Bypass` it stopped with "running scripts is disabled".
- Not checked yet: whether plain `signout` (without `--all`) closes only the calling process's session, and what happens
  when a PID is reused.
- Known overlap: the WSL watchdog (6.5) runs `signout --all` outside the Windows mutex, so when it finds a shared approval
  it can close the session of a Windows call that is running at that moment. It is rare (a shared approval exists only
  when someone approved a prompt without the window or ran `op.exe` directly) and the README mentions it.

## 2. Names

| Thing | Name |
| --- | --- |
| Repository, Rust package, executable | `op-reason` |
| Command typed every day | `op --reason "<why>" <op arguments…>` (`op` is a symlink to `op-reason`, placed first in `PATH`) |
| Log viewer | `op-reason-log` (same executable, chosen by `argv[0]`), also `op-reason --log` |
| WSL state directory | `${XDG_STATE_HOME:-~/.local/state}/op-reason/` |
| Windows state directory | `%LOCALAPPDATA%\op-reason\` |
| Environment variable prefix | `OPREASON_` |
| Windows entry | `op-reason.ps1` and the shim `op-reason.cmd` |
| Launcher | `launcher.ps1` (shared by both entries) |

The name says that the wrapper requires a reason, and matches the words the author's dotfiles already use
(`op-reason.nix`, `op-reason-guard.sh`). `op-gate`/`opgate` were avoided because other tools in the same field use them.

**Environment variable prefix (implementer's call).** `OPREASON_`, not `OP_REASON_`: the 1Password CLI reads many
variables that start with `OP_` (`OP_SERVICE_ACCOUNT_TOKEN`, `OP_CONNECT_HOST`, `OP_BIOMETRIC_UNLOCK_ENABLED`,
`OP_FORMAT`, `OP_ACCOUNT`, `OP_SESSION_*`, …), and the wrapper itself strips `OP_*` items from `WSLENV` before its
watchdog calls. A prefix that starts with `OP_` would read as a 1Password setting and would be stripped by that rule.

## 3. Architecture (plan A')

```
                    WSL                                              Windows
 ┌───────────────────────────────────────┐     ┌──────────────────────────────────────────────────────┐
 │ op-reason (Rust, one static binary)   │     │ op-reason.cmd ─▶ op-reason.ps1 (PowerShell entry)     │
 │  parse args, caller facts, flock,     │     │  parse args, caller facts, log                       │
 │  pipes, watchdog, log                 │     └───────────────┬──────────────────────────────────────┘
 └──────────────┬────────────────────────┘                     │ new powershell.exe per call
                │ new powershell.exe per call                   ▼
                ▼                                ┌──────────────────────────────────────────────────────┐
 ┌──────────────────────────────────────────────▶│ launcher.ps1 (Windows PowerShell 5.1)                │
                                                 │  named mutex, window + toast, op.exe, signout --all,  │
                                                 │  status JSON                                         │
                                                 └──────────────────────────────────────────────────────┘
```

- **WSL side: one Rust executable** for Linux x86_64 and aarch64 (statically linked, musl). It does not depend on jq,
  flock or the bash version. One executable holds:
  - the wrapper (called as `op` through a symlink, or as `op-reason`),
  - `op-reason --skill`: a skill file (SKILL.md) for AI agents, mentioned by `--help` and by refusal messages,
  - `op-reason-log` (or `op-reason --log`): the log viewer,
  - `op-reason --completion fish`: the fish completion,
  - the embedded launcher, passed to `powershell.exe` through an environment variable on every call. The launcher is
    not passed with `-EncodedCommand`: as a command line it would exceed the 32767-character limit of Windows.
- **Windows side: no executable.** A PowerShell entry (`op-reason.ps1`) and a `.cmd` shim that calls it. No code signing
  and no Smart App Control reputation wait is needed for scripts.
- **The launcher is shared.** Both entries start a fresh `powershell.exe` running the launcher for every call, which
  confines the approval to that call (on Windows too: one process starting `op.exe` twice reused the approval).
- **The lock lives in the launcher** as a Windows named mutex, so that calls from WSL and from Windows run one at a
  time. The WSL flock and its pipe handling stay (two `op` calls connected by a pipe must not deadlock); the "do not wait
  for the lock" decision is passed to the launcher.
- **`signout --all` stays** at the end of each call. It also closes approvals of other processes (another op-reason call
  running at the same time, or an `op` typed by hand in another tab); the README says so. The mutex keeps op-reason calls
  from overlapping.
- **The window and toast texts are built by the launcher** from facts that the entry passes (time, caller, place,
  reason, command). The language is English or Japanese, chosen by an environment variable.
- **The log format stays** (`v: 1`; records `start`, `end`, `refused`, `shared-approval` with the same fields), because
  the author's Claude Code hook reads the log to detect denials and timeouts. Windows calls write the same format.

### 3.1 Parallel experiment C'

Plan C' (the whole Windows side as one Rust `op-reason.exe`: entry, lock, window, `op.exe`, signout, log) is built by
another implementer on top of PR 1, as a draft pull request next to PR 2, so that the two can be compared. Unsigned
executables can be stopped by Smart App Control until each new version earns reputation, so the default Windows entry of
this design stays PowerShell.

To make C' possible:

- the Rust code is split into a **core library that builds for Windows targets too** (payload, log records, caller and
  place facts, argument planning, messages) and a WSL binary that holds the WSL-only parts (`/proc` walking, `WSLENV`,
  flock, pipes, the watchdog);
- the contracts between entries and the launcher (section 5) are fixed in this document before the code, so that an
  executable can follow them (same mutex name, same log format and location, same exit codes; it may also accept the
  launcher payload).

## 4. Command line

### 4.1 Invocation names

The executable looks at the file name of `argv[0]`:

| Name | Behavior |
| --- | --- |
| `op-reason-log` | log viewer (section 6.8) |
| `op-reason` | wrapper; the wrapper's own options below are recognized |
| anything else (`op`) | wrapper, transparent: `--help` and `--version` go to `op` |

### 4.2 Wrapper options

The wrapper's own options are recognized only as the **first** argument:

| Argument(s) | Names | Result |
| --- | --- | --- |
| (none), or only `--reason …` | all | wrapper usage on stdout, exit 0; no `powershell.exe`, no `op.exe`, no log record |
| `--help`, `-h` as the only argument | `op-reason` | wrapper usage on stdout, exit 0 |
| `--version`, `-V` as the only argument | `op-reason` | `op-reason <version>` on stdout, exit 0 |
| `--skill` (only argument) | all | the skill file on stdout, exit 0 |
| `--completion fish [<command>]` | all | the fish completion for `<command>` (default `op`) on stdout, exit 0; other shells: exit 2 |
| `--log [<log options>…]` | all | the log viewer |

Under the name `op`, `op --help` and `op --version` pass through to 1Password's help, as before. A newcomer who types
`op-reason --help` gets the wrapper's help instead.

### 4.3 The reason

- The first `--reason X` or `--reason=X` before `--` is removed from the arguments; the rest goes to `op`. It may appear
  anywhere before `--`. A `--reason` after `--` belongs to `op` (or to the child command of `op run`).
- The reason is normalized: control characters (including newline and tab) become spaces, runs of spaces become one,
  leading and trailing spaces are removed. An empty result is refused.
- Lengths (in characters): 500 in the log, 200 in the window, 100 in the toast and in the lock holder file. Longer text
  is cut and gets `…`.

### 4.4 Mode

| Condition | mode | kind |
| --- | --- | --- |
| reason given, `OP_SERVICE_ACCOUNT_TOKEN` set | `service` | `service-account` |
| reason given, `OP_CONNECT_HOST` and `OP_CONNECT_TOKEN` set | `service` | `connect` |
| reason given | `desktop` | `desktop` |
| no reason, and the call is help (first argument is `--version`, `-v`, `--help`, `-h` or `help`, or `--help`/`-h` appears before `--`) | `help` | `help` |
| no reason, anything else | refused (`no-reason`, exit 2) | |

`help` calls go through a fresh launcher too (no window); they need no approval.

### 4.5 Masking

The log, the window and the toast show arguments masked; `op` receives them unchanged:

- the values of `--session` and `--secret-key` (`--session X`, `--session=X`) become `***`;
- `name=value` arguments that do not start with `-` show `name=***` (`op item create password=…`);
- `op://` references are shown as they are;
- after `--` (the child command of `op run`), only the command name and value-less flags (`^-[A-Za-z0-9-]+$`) are shown;
  everything else becomes `***`.

The command is displayed as `op <masked arguments>`; an argument made only of `[A-Za-z0-9_./:=@%+,*?-]` is shown as is,
others are quoted with `"` (and `\`/`"` escaped); control characters (newline, tab, escape, …) become spaces, so that
an argument cannot move the cursor or rewrite a terminal, the window or the toast. The log viewer also shows control
characters of logged values as escapes (`\x1b`).

### 4.6 The call table

`tests/cases/calls.json` lists invocations and the expected plan: given `argv`, the invocation name and environment
variables, the expected action (`usage`, `version`, `skill`, `refuse`, `run`), the refusal reason and exit code, the mode,
the kind, the arguments `op` receives, the masked arguments, the displayed command and the normalized reason. The Rust
core tests read it; the Windows CI runs the PowerShell entry against it (PR 2). This keeps the two implementations of
the entry in step.

## 5. Contracts

This section is what C' must follow. "Entry" means the WSL binary, the PowerShell entry, or an executable that replaces
the PowerShell entry.

### 5.1 Entry → launcher: process and environment

- Command: `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -OutputFormat Text` and then
  - from WSL: `-EncodedCommand <bootstrap>`, with the launcher source in `OPREASON_SCRIPT`;
  - from the PowerShell entry: `-File <path of launcher.ps1>`.
- `OPREASON_SCRIPT`: base64 of the UTF-8 launcher source (byte order mark removed). It must stay below 30000 characters
  (an environment variable holds at most 32767); a test checks it.
- The bootstrap (UTF-16LE, base64): sets `$ProgressPreference = 'SilentlyContinue'` (otherwise PowerShell writes CLIXML
  progress records to stderr), reads and removes `OPREASON_SCRIPT`, decodes it as UTF-8, and runs it in the same scope
  (`. $scriptblock`) so that the launcher's `exit` becomes the process exit code. On failure it writes one
  `op-reason: launcher: cannot load the script: …` line to stderr and exits 1.
- `OPREASON_PAYLOAD`: the payload JSON (5.2), written with ASCII only (non-ASCII as `\uXXXX`). The launcher removes it
  from its environment before `op.exe` starts. From WSL both variables are added to `WSLENV`.
- Standard input, output and error of `op.exe` are the launcher's own (no redirection, no PowerShell pipeline touches
  the bytes), unless the payload asks for `stderr: "scan"` (PR 2, Windows entry).
- Service mode from WSL: `OP_SERVICE_ACCOUNT_TOKEN` (or `OP_CONNECT_HOST` and `OP_CONNECT_TOKEN`) and
  `OP_BIOMETRIC_UNLOCK_ENABLED=false` are passed with `/w` in `WSLENV`. Desktop and help modes remove all three token
  variables from the environment.

### 5.2 Payload

```json
{
  "v": 1,
  "mode": "desktop",
  "op": "C:\\Users\\me\\AppData\\Local\\Microsoft\\WinGet\\Links\\op.exe",
  "args64": ["aXRlbQ==", "Z2V0"],
  "status": "\\\\wsl.localhost\\Ubuntu\\home\\me\\.local\\state\\op-reason\\run\\<id>\\status.json",
  "lang": "en",
  "lock": "held",
  "lockWait": 90,
  "stderr": "inherit",
  "facts": {
    "time": "2026-11-15 12:34:56",
    "caller": { "type": "claude", "pid": 999, "session": "…", "detail": null },
    "herdr": { "workspace": "w1", "label": "my workspace", "pane": "w1:p2" },
    "place": { "repo": "owner/repo", "branch": "main", "cwd": "~/src/repo" },
    "reason": "read the API token to deploy the staging VM",
    "command": "op read op://vault/item/field",
    "notes": [ { "kind": "shared-cleared" }, { "kind": "lock-skipped", "holderReason": "…" } ]
  }
}
```

| Field | Meaning |
| --- | --- |
| `v` | contract version, `1` |
| `mode` | `desktop`, `help`, `service` or `notify` |
| `op` | Windows path of `op.exe` (not used by `notify`) |
| `args64` | arguments for `op.exe`, each UTF-8 then base64 (PowerShell 7's `ConvertFrom-Json` would turn date-like plain strings into `DateTime`) |
| `status` | path where the launcher writes the status JSON (optional) |
| `lang` | `en` or `ja` (missing: `en`) |
| `lock` | `held`: take the global lock (the named mutex, PR 2); `skipped`: do not (a pipe: the lock holder is waiting for this call's output). Missing: `held` |
| `lockWait` | seconds to wait for the mutex (PR 2). Missing: 90 |
| `stderr` | `inherit` (default) or `scan` (PR 2: the launcher relays `op.exe`'s stderr and reports whether it saw a denial or a timeout) |
| `facts` | desktop mode: what the window shows; the launcher builds the texts |
| `notify` | notify mode: `{ "kind": "shared-approval", "when": "start" \| "end", "cleared": true \| false, "time": "HH:MM:SS" }` |

`facts` is sent in `desktop` mode only (help and service calls show no window). Its fields: `time` (local, `YYYY-MM-DD HH:MM:SS`), `caller.type` (`claude` or `human`), `caller.pid` (the Claude Code
process, or null), `caller.session` (`CLAUDE_CODE_SESSION_ID` or null), `caller.detail` (free text such as a process
chain, or null), `herdr` (null outside herdr; `label` may be empty), `place.repo` and `place.branch` (null outside a git
repository), `place.cwd` (with `~` for the home directory), `reason` (normalized, up to 200 characters), `command`
(masked display, up to 300 characters), `notes` (`shared-cleared`: a shared approval was found and cleared before the
call; `lock-skipped`: this call did not wait for the lock, with the holder's reason).

### 5.3 Modes in the launcher

| Mode | Lock (PR 2) | Window and toast | `op.exe` | After `op.exe` |
| --- | --- | --- | --- | --- |
| `desktop` | yes, unless `lock: skipped` | yes | yes | close sessions: `whoami`, then `signout --all` only when signed in; nothing when `lock: skipped` |
| `help` | yes, unless `lock: skipped` | no | yes | close sessions as above |
| `service` | yes, unless `lock: skipped` | no | yes (tokens inherited) | nothing |
| `notify` | no | toast only | no | nothing |

`whoami` first, because it does not prompt when there is no session; plain `signout` would close only the most recently
used account.

A call with `lock: skipped` (the sender of a pipe whose receiver holds the lock) does not close sessions (status
`signout: "skipped"`): `signout --all` would also close the receiver's session while the receiver's `op.exe` is still
running. The receiver normally ends later (it reads the sender's output to the end) and its own `signout --all` closes
the sender's session too. If the receiver ends first (it did not read its input), the sender's session stays bound to
the sender's launcher, which has exited; it is not usable by other processes and expires with 1Password's time limit.

### 5.4 Launcher → entry: status and exit code

The launcher writes the status JSON (UTF-8 without byte order mark) at the end, when `status` is given:

```json
{"started":true,"exit":0,"error":"","window":"shown","toast":"shown","others":0,"signout":"ok","lock":"held","ui":{…}}
```

(`lock`, `holder` and `stderrMatch` are written from PR 2 on; a missing field reads as null.)

| Field | Values |
| --- | --- |
| `started` | whether `op.exe` was started |
| `exit` | `op.exe`'s exit code, or null |
| `error` | empty, or a short message; `lock-timeout` when the mutex wait timed out (PR 2) |
| `window` | `shown`, `skipped`, `failed: …` |
| `toast` | `shown`, `skipped`, `failed: …` |
| `others` | the largest number of other `op.exe` processes seen while the window was open |
| `signout` | `ok`, `not-signed-in`, `skipped`, `failed: …` |
| `lock` | `held`, `skipped`, `timeout` (PR 2) |
| `holder` | PR 2, on `lock-timeout`: who holds the mutex, as one line of text (time, caller, reason, command) |
| `stderrMatch` | PR 2, `stderr: "scan"` only: `denied`, `timeout` or null |
| `ui` | the texts the launcher built (title, lines, footer, others, toast), for tests |

Exit code: `op.exe`'s exit code; 127 when `op.exe` is missing (and one `op-reason: launcher: op.exe not found: …` line on
stderr); 1 for other failures before `op.exe` starts; 75 when the mutex wait timed out (PR 2); `notify` exits 0. The
launcher writes nothing else to stdout or stderr (PR 2 may add one "waiting for another op-reason call" line on stderr).

### 5.5 The global lock

- **WSL flock**: `<state>/lock`, taken by the WSL binary before the watchdog and the launcher. While waiting (up to
  `OPREASON_LOCK_WAIT` seconds, default 90 — shorter than the 120-second default timeout of Claude Code's Bash tool), it
  checks every second for the pipe cases of 6.4. `<state>/current.json` holds the holder:
  `{"time":"HH:MM:SS","caller":"<short caller>","reason":"<100 chars>","command":"<100 chars>","pid":<pid>}`.
- **Windows named mutex (PR 2)**: `Local\op-reason`, taken by the launcher in all modes except `notify`, unless the
  payload says `lock: skipped`. It waits up to `lockWait` seconds; an abandoned mutex counts as acquired. While holding
  it, the launcher writes `%LOCALAPPDATA%\op-reason\holder.json` (same fields as `current.json`, `pid` being the
  launcher's). It releases the mutex after closing the sessions. On timeout: no `op.exe`, status
  `{"started":false,"error":"lock-timeout","lock":"timeout","holder":"…"}`, exit 75.
- A WSL call passes `lockWait` = what is left of its own `OPREASON_LOCK_WAIT` after waiting for the flock (at least 1).
- An executable that replaces the Windows entry (C') takes the same mutex around its whole call.

### 5.6 Log

Files: `<state>/log.jsonl` (WSL: `${XDG_STATE_HOME:-~/.local/state}/op-reason/`; Windows:
`%LOCALAPPDATA%\op-reason\`). When it reaches 1 MiB it is renamed to `log.1.jsonl` (one generation). Appending and
rotating take an exclusive lock on `<state>/log.lock` (Linux: `flock`; Windows: `LockFileEx` on byte 0 or a wider
range; Rust's `File::lock` does this on both), waiting up to 5 seconds and writing anyway after that.

Lock files (`log.lock`, and `lock` on WSL) are opened for reading and writing, never for appending only: Windows refuses
`LockFileEx` on an append-only handle.

Records (one JSON object per line, `v: 1`; `time` is local ISO 8601 with offset, `epoch` is UNIX seconds; `id` is
`YYYYmmddTHHMMSS-<pid>-<random>` and is shared by a call's `start` and `end`):

```json
{"v":1,"id":"…","event":"start","time":"…","epoch":0,"session":null,"mode":"desktop","kind":"desktop","lock":"held","caller":{…},"reason":"…","args":["…"]}
{"v":1,"id":"…","event":"end","time":"…","epoch":0,"session":null,"result":"approved","exit":0,"seconds":3,"window":"shown","toast":"shown","others":0,"signout":"ok","watch":{"start":"none","end":"none"}}
{"v":1,"id":"…","event":"refused","time":"…","epoch":0,"session":null,"why":"no-reason","caller":{…},"reason":"","args":["…"]}
{"v":1,"id":"…","event":"shared-approval","time":"…","epoch":0,"when":"start","cleared":true}
```

- `caller`: `{"type":"claude"|"human","claude_pid":<n>|null,"herdr":{"workspace":…,"label":…,"pane":…}|null,"cwd":…,"repo":…|null,"branch":…|null}`
- `session`: `CLAUDE_CODE_SESSION_ID` or null. `args`: masked. `reason`: up to 500 characters.
- `start.lock`: `held` or `skipped`.
- `end.result`: `approved` (desktop, exit 0), `ok` (help and service, exit 0), `denied`, `timeout`, `failed`,
  `launcher-failed` (no status, or `started` false), `interrupted` (stopped by a signal), and from PR 2
  `lock-timeout` (the mutex wait timed out; exit 75). `window`, `toast`, `others`, `signout` come from the status (null
  when missing). `watch` is the WSL watchdog's result at start and end (`none`, `cleared`, `error`, or null when not run).
- `refused.why`: `no-reason`, `too-long`, `lock-timeout`, `no-op-exe`, `no-powershell`, `no-wslpath`, `interrupted`
  (stopped before start). (`no-launcher` from the bash version can still appear in old logs.)
- Never written: `op`'s output, tokens, unmasked secret values.
- Denial and timeout are recognized from `op`'s stderr lines (case-insensitive): `^\[ERROR\] .*authorization prompt dismissed`
  → denied; `^\[ERROR\] .*authoriz.*(timeout|timed out)` → timeout. Only `op`'s `[ERROR]` lines count (a child command's
  "Connection timed out" does not).

### 5.7 Exit codes of the wrapper

| Code | Meaning |
| --- | --- |
| `op`'s code | the call ran |
| 2 | no reason, empty reason, arguments too long (base64 of the arguments over 28000 characters, or a payload over 32000), unknown shell for `--completion` |
| 75 | the lock wait timed out (another call is waiting for its approval) |
| 127 | `op.exe`, `powershell.exe` or `wslpath` not found, or `powershell.exe` could not be started (a message says why) |
| 1 | the launcher failed before `op.exe` started |
| 129, 130, 143 | stopped by SIGHUP, SIGINT, SIGTERM (WSL) |

### 5.8 Language

`OPREASON_LANG` = `en` or `ja` chooses the language of stderr messages, usage, the window, the toast and the log viewer.
When it is unset or empty: WSL looks at `LC_ALL`, `LC_MESSAGES`, `LANG` (the first non-empty one; `ja…` → Japanese);
the PowerShell entry looks at the UI culture. Anything else is English. The entry resolves the language once and passes
it to the launcher as `lang`.

## 6. WSL binary

### 6.1 Flow of one call

1. Parse the arguments (4.2, 4.3). No `op` arguments → usage, exit 0.
2. Mask the arguments, build the displayed command.
3. Caller facts: the nearest ancestor whose `comm` is `claude` (walking `/proc/<pid>/stat` up to 64 levels) → `claude`
   with its pid, otherwise `human`. Inside herdr (`HERDR_ENV=1`): workspace id and pane from `HERDR_WORKSPACE_ID` and
   `HERDR_PANE_ID`, and the workspace label from `herdr workspace get <id>` (2-second timeout; optional). Place: repo and
   branch from the files of the git repository that contains the current directory (6.6).
4. Mode (4.4). Refusals are logged and printed with a short usage that mentions `op-reason --skill` for agents.
5. Tools: `op.exe` (`OPREASON_OP_EXE`, or `op.exe` in `PATH`; must be a file), `powershell.exe` (`OPREASON_POWERSHELL`,
   or `powershell.exe` in `PATH`), `wslpath`. Missing → refused, exit 127.
6. Too long: base64 of the arguments over 28000 characters → refused (`too-long`), exit 2.
7. Signals (SIGINT, SIGTERM, SIGHUP) are handled from here, on a dedicated thread: after `start`, an `end` with
   `interrupted`; before, a `refused` with `interrupted`; exit 130/143/129. Temporary files are removed. Every child
   starts with an empty signal mask (the standard library does not reset it), so a TERM or HUP to the process group
   reaches the launcher; a TERM or HUP to the wrapper alone is forwarded to the launcher. A signal after the `end`
   record only exits.
8. Pipes (6.4): if another op-reason feeds our stdin, read stdin to the end first.
9. Lock (5.5). Timeout → refused (`lock-timeout`) with the holder from `current.json`, exit 75.
10. Remove leftovers of calls killed by SIGKILL (`<state>/run/*` older than an hour). Write `current.json` when holding
    the lock.
11. Watchdog at start (6.5), unless the lock was skipped.
12. Log `start`. Build the payload (5.2) and start the launcher (5.1): stdin is ours (or the drained copy), stdout is
    ours, stderr is relayed to ours while the last 256 KiB are kept to classify the result.
13. Result from the status file and `op`'s exit code (5.6). Watchdog at end, unless the lock was skipped. Log `end`.
14. Exit with `op`'s exit code.

### 6.2 State directory

`<state>` = `${XDG_STATE_HOME:-~/.local/state}/op-reason/` (mode 0700): `log.jsonl`, `log.1.jsonl`, `log.lock`, `lock`,
`current.json`, `run/<id>/status.json` (the launcher writes it through the `\\wsl.localhost\…` path, so it is not on a
tmpfs). Drained stdin and the stderr copy stay in memory; nothing secret is written to disk.

### 6.3 Windows paths

`wslpath -w` converts the `op.exe` path and the status path.

### 6.4 Pipes between two calls

`op … | op …` (also with other commands in between) must not deadlock: the receiver holds the lock while waiting for the
sender's output, and the sender waits for the lock.

- **Receiver**: if a process up the pipe chain of our stdin (up to 4 levels; the writer of a pipe is a process whose
  `/proc/<pid>/fd/1` is the same pipe) is op-reason (its `/proc/<pid>/exe` is the same file as ours), read stdin to the
  end before taking the lock. Check again every second while waiting (the sender may start later).
- **Sender**: while waiting, check every second whether the lock holder (`current.json` `pid`, an op-reason process)
  reads our output up its pipe chain. If so, run without the lock: start `lock` = `skipped`, payload `lock: skipped`
  (the launcher neither takes the mutex nor signs out), a `lock-skipped` note in the window, and no watchdog (the
  holder runs it).
- Other pipes (which may never close, such as ssh) and sockets (Claude Code's Bash stdin) are not drained.

### 6.5 Shared approval watchdog

From WSL, `op.exe whoami` is run directly (30-second timeout, stdin/stdout/stderr discarded, token variables removed,
`OP_*` items removed from `WSLENV`). If it succeeds, an approval usable from every pane exists (someone approved a prompt
without the window, or a tool ran `op.exe` directly): run `op.exe signout --all`, check `whoami` again, log
`shared-approval` (`cleared` true/false), and notify (the launcher in `notify` mode, started in the background, and
`herdr notification show` inside herdr). Without a session, `whoami` does not prompt.

### 6.6 Place (repo and branch) without git

Shared by both entries (the PowerShell entry reads the same files, since Windows may have no `git.exe`):

1. From the current directory up, find `.git`. A file `.git` holds `gitdir: <path>` (worktrees, submodules); the git
   directory may have a `commondir` file that points to the shared directory.
2. `HEAD`: `ref: refs/heads/<branch>` → branch; another `ref:` → the ref without `refs/heads/`; a hash →
   `(detached <first 7>)`.
3. Origin: `url` in the `[remote "origin"]` section of `<common dir>/config`. Remove a trailing `/` and `.git`; owner =
   the path segment before the last `/` (after `:` or `/`); repo = `owner/name`. Without an origin: the name of the
   directory that holds `.git`.
4. Displayed place: `<repo> (<branch>) · <cwd>`, or `<cwd>` outside a repository.

### 6.7 Messages

stderr lines start with `op-reason: `. Usage texts and messages exist in English and Japanese (5.8). The short usage
printed with a refusal ends with a line for AI agents: run `op-reason --skill` for the full guide.

### 6.8 Log viewer

`op-reason-log [--since <n>(s|m|h|d)] [--all] [--json]`, default the last hour, by start time. It reads `log.1.jsonl` and
`log.jsonl`, skips lines that are not JSON, merges each `start` with its `end` (by `id`), and prints a table (time,
result, seconds, caller, reason, command) or, with `--json`, one JSON object per call (the start with an `end` field).
The time is `HH:MM:SS` today and `MM/DD HH:MM:SS` on other days. Results get notes: `(SA)`, `(Connect)`, `(help)`, `(no
window)` when the window failed, `(lock skipped)`. A start without an end is "running or interrupted". Columns are cut
(caller 40, reason 48, command 60 characters) and aligned by display width. `OPREASON_LOG_NOW` (UNIX seconds) overrides
the clock for tests. Exit 0 (also when there is no log), 2 on a usage error.

### 6.9 Skill file

`op-reason --skill` prints a SKILL.md (frontmatter `name: op-reason` and a description) for AI agents, written in
English and independent of the author's environment: how to call (`op --reason "<one sentence: what for, what is read
or written>" …`), that one approval covers one call (put everything a task needs into as few calls as possible), long
tool timeouts (an approval can take minutes), never calling `op.exe` directly, help and version need no reason, what to
do on each exit code (denied or timed out: do not retry, ask the user with options; 75: another call is waiting for an
approval; 2: add a reason), not printing secrets, avoiding long-running `op run` (the lock and the window stay until it
exits), and `op-reason-log`.

### 6.10 Fish completion

`op-reason --completion fish [<command>]` prints the completion (ported from the dotfiles, comments in English, command
name substituted): `--reason`, `op` commands two levels deep, global flags, flags of common commands, files where `op`
takes files, and the child command's own completion after `--`. It never runs `op` (fish's bundled completion runs `op
completion fish`, which the wrapper would refuse). Install it as `~/.config/fish/completions/op.fish` (and
`op-reason.fish` with `op-reason` as the command).

## 7. Windows entry (PR 2)

- `op-reason.ps1` (UTF-8 with byte order mark: Windows PowerShell 5.1 reads files without one in the ANSI code page) and
  `op-reason.cmd` (`powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0op-reason.ps1" %*`,
  with `OPREASON_ENTRY=cmd` and `OPREASON_ENTRY_NAME=%~n0` set for the entry).
- Arguments: from PowerShell they come from PowerShell (`$args`); PowerShell takes an unquoted `--` for itself, so the
  README says to write `'--'`. Through the shim the entry splits the process command line itself with the C runtime's
  rules (PowerShell's `-File` binding drops empty arguments and `--`).
- The `.cmd` shim goes through cmd.exe, which re-parses the arguments: unquoted `&`, `|`, `%` and similar characters
  can break (the root of "BatBadBut"). The README says so, and says that shells whose execution policy allows scripts
  (such as Claude Code's tool shell, which runs with `Bypass`) can call `op-reason.ps1` directly.
- Same planning as the WSL binary (the call table), same log format in `%LOCALAPPDATA%\op-reason\`.
- Caller: Claude Code from `CLAUDECODE`/`CLAUDE_CODE_SESSION_ID`/`CLAUDE_PID`, the Windows Terminal tab from
  `WT_SESSION`, and the process chain as `detail`. herdr facts when `HERDR_ENV=1` and `herdr` is available.
- Place: 6.6 (no `git.exe` needed).
- Each call starts a fresh `powershell.exe` with the launcher (`-File`), payload `lock: held`, `lockWait` =
  `OPREASON_LOCK_WAIT` (default 90), `stderr: scan` (the entry cannot tee stderr itself).
- When started from a PowerShell session, the launcher runs as a native command so that output can be captured
  (`$x = op-reason …`, decoded as UTF-8; the console encoding and the environment are put back afterwards); when
  started by the `.cmd` shim, standard handles are inherited (binary-safe, checked in the Windows CI).
- A call stopped by Ctrl+C (or a closed console) is logged as `end` `interrupted` with exit 130 when PowerShell runs
  the entry's cleanup (`finally`); the PowerShell process's own exit code is PowerShell's. (The Windows executable of
  C' returns 130 for Ctrl+C and 129 for a closed console.)
- The holder of the mutex is shown from `holder.json` in the call's language; a timeout prints "Another op-reason call
  (from Windows or WSL) is still waiting for its approval" with the holder and exits 75.
- No shared-approval watchdog (there is no shared parent process on Windows).
- `op-reason … | op-reason …` is not detected on Windows (no `/proc`): the second call may wait for the mutex until
  `lockWait` runs out. The README says so (store the first output in a variable instead).
- `--skill` prints the bundled SKILL.md; no log viewer on Windows in PR 2 (the README shows a PowerShell one-liner).
- Distributed as a zip on the GitHub Release: `op-reason.ps1`, `op-reason.cmd`, `launcher.ps1`, `SKILL.md`; the user
  puts the folder in `PATH`.

## 8. Launcher

Changes from the dotfiles version:

- environment names `OPREASON_SCRIPT` and `OPREASON_PAYLOAD`; message prefix `op-reason: launcher:`;
- `OPREASON_SCRIPT` carries the source without the byte order mark, comment lines and blank lines (the launcher has
  no multi-line strings), which keeps it well below the limit of an environment variable;
- no `signout --all` for `lock: skipped` calls (5.3);
- texts built from `facts` in English or Japanese (`lang`); the file is UTF-8 with a byte order mark and may contain
  Japanese text (the WSL path decodes it as UTF-8; `-File` reads the byte order mark);
- `ui` in the status (the texts built), so that tests on Linux can check them;
- the texts live in `Get-Texts`, one `key = 'value'` per line for each language (C' reads this table to check that
  its own texts match; a change of this shape is a contract change);
- PR 2: the named mutex and `holder.json`, `stderr: scan`.

Kept: Windows PowerShell 5.1 syntax (no `??`, `?.`, ternary, pipeline chains, `$IsWindows`), arguments quoted the way
Go's `syscall.EscapeArg` does (so `op.exe` and .NET split them back unchanged), `op.exe` started with
`ProcessStartInfo` without redirection and `CreateNoWindow = false` (with true the child gets no stdin), the window
(topmost, not activated, top right of the work area so that it does not cover the 1Password prompt, never asks
anything), the count of other `op.exe` processes every 250 ms, the toast through Windows PowerShell's own
AppUserModelID, and nothing on stdout/stderr besides the one error line.

## 9. Code layout

```
Cargo.toml                    workspace
crates/op-reason-core/        library, builds for Linux and Windows
  args.rs                     reason extraction, normalization, clipping, masking, display, mode planning
  contract.rs                 payload, facts, status, constants (env names, mutex name, exit codes)
  log.rs                      records, append with rotation under log.lock
  logview.rs                  the log viewer (parsing, merging, table, JSON)
  place.rs                    repo and branch from git files
  messages.rs                 English and Japanese texts
  result.rs                   result classification
  launcher.rs                 embedded launcher, OPREASON_SCRIPT and the bootstrap
  base64.rs, json.rs, time.rs small helpers (ASCII JSON, local time)
crates/op-reason/             the WSL binary (Linux)
  main.rs                     argv[0] dispatch and wrapper options
  wrapper.rs                  the flow of 6.1
  procfs.rs                   /proc: Claude ancestor, pipe writers, op-reason detection
  lock.rs                     flock waiting with the pipe checks
  signals.rs                  SIGINT/SIGTERM/SIGHUP on a dedicated thread (sigwait)
  launch.rs                   WSLENV, wslpath, starting the launcher, stderr relay, timeouts
launcher/launcher.ps1         the shared launcher
windows/                      PR 2: op-reason.ps1, op-reason.cmd
completions/op.fish           fish completion template
skill/SKILL.md                the skill file
tests/                        black-box tests (bash, pwsh, fish) and tests/cases/calls.json
nix/flake.nix                 development tools (rust, pwsh, fish, jq, shellcheck, shfmt)
flake.nix                     the package (for Nix users and the author's dotfiles)
```

Dependencies are kept small: `serde` (derive) and `serde_json` for the contract types, `chrono` (local time, clock only),
`libc` (signals, `isatty`) on Unix. Base64, ASCII JSON and display width are a few lines each. `File::lock` (Rust 1.89)
replaces flock(1). No dependency newer than 7 days at the time it is added (crates.io has no release-age gate; publish
dates are checked by hand).

## 10. Tests and CI

- **Black-box tests ported from the dotfiles** (`wrapper`, `launcher`, `e2e`, `log`, `completion`), run against the
  Rust binary with fake `powershell.exe`, `op.exe`, `wslpath` and `herdr`. They check the same behavior: reasons,
  help without reason, usage, binary stdin/stdout, stderr, exit codes, payload and `WSLENV`, service and Connect, masking,
  results, caller facts, lock timeout, the shared approval watchdog, missing tools, signals, log rotation and cleanup,
  pipes in all start orders. Tests of temporary files that no longer exist (the stderr copy, `XDG_RUNTIME_DIR`) are
  replaced by a check that no file is left behind.
- **Launcher tests on Linux pwsh** (as in the dotfiles) plus the text building in both languages.
- **Rust unit tests** for the core, including the call table.
- **CI (GitHub Actions)**: Linux (in the `nix/` development shell): format, clippy, unit tests, black-box tests,
  shellcheck, and the flake lock age check. Windows: build and test the core library (for C'). PR 2 adds the Windows
  runner jobs: the launcher and the PowerShell entry on real Windows PowerShell 5.1 with a fake `op.exe`, the call table,
  and a screenshot of the window attached to the pull request.
- The real 1Password app cannot run in CI. A manual check list (`docs/manual-verification.md`, Japanese version too) is
  run by the author.

## 11. Distribution

- GitHub Releases: Linux x86_64 and aarch64 binaries (musl, static) with SHA-256 files; PR 2 adds the Windows zip. A
  workflow builds them on a `v*` tag. Tags and releases are made after merge, by the maintainer.
- Nix flake: `packages.default` (`op-reason`, `op-reason-log`, fish completion for `op-reason`) and `packages.op` (adds
  the `op` symlink and the fish completion for `op`). The author's dotfiles will take the flake as an input.
- Not published to crates.io, Homebrew or apt.

## 12. Documents

- `README.md` (English) starts with a link to `README.ja.md` (Japanese). It says that op-reason is unofficial (not made,
  endorsed or supported by 1Password/AgileBits).
- Code comments in English. Window and stderr texts in English and Japanese.
- `docs/manual-verification.md` and `docs/manual-verification.ja.md`: checks with the real 1Password app.
- License: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`, copyright holder `pollenjp`).

## 13. Out of scope (decided later)

- Replacing the dotfiles package with this flake (done in the dotfiles repository).
- A richer window (a separate pull request, so that behavior changes and visual changes can be reviewed apart).
- An article and a video.
- Moving the Claude Code hook and other helpers of the dotfiles.
- No dependency on the dotfiles: herdr facts are optional; no dotfiles configuration files or home-manager modules.

## 14. Implementer's decisions

| Decision | Why |
| --- | --- |
| Prefix `OPREASON_` | see section 2 |
| `--help`/`--version` are the wrapper's only under the name `op-reason` | keep `op --help` transparent, give newcomers the wrapper's help |
| Wrapper options only as the first argument | never shadow `op` flags in the middle of a command line |
| Fish completion takes the command name | one template for `op` and `op-reason` |
| `op-reason-log` by `argv[0]`, also `--log` | works without a symlink |
| Drained stdin and stderr copy in memory | no secret on disk; no FIFO |
| `/proc/<pid>/exe` to recognize op-reason | robust for a binary (the bash version compared the script path) |
| Payload in ASCII JSON, total ≤ 32000 | Windows environment variables hold 32767 UTF-16 units |
| Launcher may contain Japanese (UTF-8 with BOM) | texts are built in the launcher; both start paths read UTF-8 |
| Status carries the built `ui` texts | tests on Linux can check the texts without WPF |
| Mutex wait timeout logs `end` with `lock-timeout` | `start` is already written when the launcher waits; the hook can learn the new value |
| Language from `OPREASON_LANG`, then the locale | English by default for an OSS tool, Japanese for Japanese locales |
| Workspace with a core library and a WSL binary | C' can build its executable on the core |
| Rust edition 2024, MSRV 1.89 | `File::lock` |
| Development tools in `nix/` (read with `path:`), the package flake at the root | the author's conventions |
| No log viewer on Windows in PR 2 | keep PR 2 to the entry; the format is plain JSON Lines |
| `lock: skipped` calls do not sign out (5.3) | found by the builder check: the sender's `signout --all` would close the receiver's session mid-call; the receiver's final `signout --all` cleans up |
| Comment lines are stripped from `OPREASON_SCRIPT` | the launcher with Japanese texts is about 28000 characters of base64 with comments; PR 2 adds code |
