# op-reason

日本語版は [README.ja.md](README.ja.md) にあります。

> **Unofficial.** op-reason is an independent project. It is not made, endorsed or supported by 1Password
> (AgileBits Inc.). 1Password is a trademark of AgileBits Inc.

op-reason is a wrapper for the [1Password CLI](https://developer.1password.com/docs/cli/) (`op`) on WSL and Windows.
Every call

1. must give a reason: `op --reason "<why>" <op arguments…>`,
2. gets its own approval from the 1Password desktop app, confined to that one call,
3. shows a window with who calls, where, why and the command, next to 1Password's approval prompt, and
4. is recorded in a log (`op-reason-log`).

![Animation: in a WSL terminal, Claude Code runs op --reason "read the Proxmox API token to create the staging VM" read "op://Private/proxmox/token". op-reason's window opens at the top right of the screen, dark: the reason first, in large letters; the caller (Claude Code and its process id), the repository with its branch and the directory as chips; the command in a monospace box; a band that marks where 1Password's prompt usually opens; and the 1Password approval prompt (drawn as a plain box) opens in the middle of the screen. After a click on Approve, op prints the value (hidden here), op-reason signs out and its window closes, and op-reason-log shows the call as one line: approved, by Claude, with the reason and the command.](docs/images/readme-1-call.en.svg)

It was written for AI coding agents (such as Claude Code) that run `op` on your behalf: you see what each request is
for before you approve it, one approval never covers later requests, and you can review every call afterwards.

op-reason can stand in front of the 1Password SSH agent too, so that every use of an SSH key says who asks and why:
see [SSH keys: the relay](#ssh-keys-the-relay).

## Why a wrapper

- **One approval leaks to later calls.** The 1Password desktop app binds a CLI approval to the process that started
  `op.exe`. From WSL every `op.exe` is a child of the same `wslhost` that all terminals share, so a single approval
  reaches later calls and other terminals. For every call op-reason starts a fresh `op-reason.exe` on Windows, which
  starts `op.exe` and signs out at the end. Called from Windows, `op-reason.exe` is itself the fresh process of the
  call.
- **The prompt does not say who asks.** 1Password's prompt names only the terminal ("WindowsTerminal", or "wslhost"
  from WSL), whatever process asked. op-reason's window shows the caller (a person or Claude Code with its process id),
  the repository, branch and directory, the reason and the command.
- **Nothing records what agents did.** op-reason logs every call, refusal and finding as JSON Lines.

![Animation of two screens side by side. Left, op alone from WSL: terminal 1 calls op and you approve the 1Password prompt in the middle of the screen once; when terminal 2 calls op, the value comes out with no prompt at all. Right, op-reason: every call opens op-reason's window at the top right with the caller and the reason, along with the prompt, so terminal 2 is asked again and you see who asks and why.](docs/images/readme-2-approval-leak.en.svg)

## How it works

![op-reason components](docs/architecture/plantuml/out/01_overview.svg)

For every call from WSL, the WSL executable checks the reason, collects the caller and the place, and starts a fresh
`op-reason.exe --run` on Windows with the call. `op-reason.exe` takes a lock shared by WSL and Windows (one approval
prompt at a time), shows the window at the top right of the screen you work on (that of the foreground window) with a
Windows notification that goes when the window closes, starts `op.exe` with your stdin, stdout and stderr, and when
`op` exits runs `op.exe whoami` and, if a session is open, `op.exe signout --all`. Called from
Windows, `op-reason.exe` does all of this in its own process, with the same lock, window texts and log.

Where `op-reason.exe` cannot run (Smart App Control may block it: it is not signed), the PowerShell launcher
`launcher.ps1` does the same job in a fresh `powershell.exe` (its window always at the top right of the primary screen,
its notification left in the notification center): calls from WSL fall back to it by themselves, and the
PowerShell entry uses it on Windows. Details: [docs/architecture/README.md](docs/architecture/README.md).

## Requirements

- For the WSL side: WSL 2 with Windows interop (the Windows `PATH` is part of the WSL `PATH`, so `op.exe`,
  `op-reason.exe` and `wslpath` are found)
- On Windows: the 1Password desktop app with **Settings → Developer → Integrate with 1Password CLI** turned on, the
  1Password CLI (`op.exe`, for example `winget install AgileBits.1Password.CLI`), and op-reason's Windows zip (also
  for calls from WSL: they show their window through its `op-reason.exe`)
- Windows PowerShell 5.1 (part of Windows), only for the fallback (`launcher.ps1` and the PowerShell entry)

## Install

### From a release

Releases start with v0.1.0. Each release has static Linux executables for x86_64 and aarch64 and the Windows zip,
each with a SHA-256 file. **On WSL, install both**: the Linux executable below, and the Windows zip as described under
[On Windows](#on-windows) (calls from WSL show their window through its `op-reason.exe`; use the same release on both
sides).

```sh
arch=$(uname -m)   # x86_64 or aarch64
base=https://github.com/pollenjp/op-reason/releases/latest/download
curl -fLO "${base}/op-reason-${arch}-linux"
curl -fLO "${base}/op-reason-${arch}-linux.sha256"
sha256sum -c "op-reason-${arch}-linux.sha256"
install -m 755 "op-reason-${arch}-linux" ~/.local/bin/op-reason
ln -sf op-reason ~/.local/bin/op-reason-log
ln -sf op-reason ~/.local/bin/op     # optional: make `op` the wrapper (~/.local/bin must come first in PATH)
ln -sf op-reason ~/.local/bin/op-reason-ssh   # optional: ssh through the SSH relay (below)
```

The WSL executable finds `op-reason.exe` in the `PATH` (WSL adds the Windows `PATH`) or where `OPREASON_EXE` points
(a WSL path such as `/mnt/c/Users/<you>/AppData/Local/Programs/op-reason/op-reason.exe`), and `launcher.ps1` next to
it.

### With Nix

```sh
nix profile install github:pollenjp/op-reason      # op-reason, op-reason-log, op-reason-ssh
nix profile install github:pollenjp/op-reason#op   # the same, plus `op`
```

The flake also works as an input: `packages.<system>.default` and `packages.<system>.op`.

### On Windows

Each release has `op-reason-windows.zip` (with a `.sha256` file):

- `op-reason.exe`: the Windows entry, one program that does the whole call itself (the reason, the lock, the window,
  `op.exe`, the sign-out and the log), and the program that shows the window of calls from WSL;
- `launcher.ps1`: the fallback of calls from WSL;
- `powershell-entry\`: the PowerShell entry, for machines where Smart App Control blocks `op-reason.exe` (below);
- `SKILL.md`, the READMEs and the licenses.

Unpack it into one folder and add the folder to your `PATH`. In PowerShell:

```powershell
$dir = "$env:LOCALAPPDATA\Programs\op-reason"    # any folder; it is added to your PATH below
$base = 'https://github.com/pollenjp/op-reason/releases/latest/download'
$zip = "$env:TEMP\op-reason-windows.zip"
Invoke-WebRequest "$base/op-reason-windows.zip" -OutFile $zip -UseBasicParsing
Invoke-WebRequest "$base/op-reason-windows.zip.sha256" -OutFile "$zip.sha256" -UseBasicParsing
$want = (Get-Content "$zip.sha256").Split(' ')[0]
if ((Get-FileHash $zip -Algorithm SHA256).Hash -ne $want) { throw 'the SHA-256 does not match' }
Expand-Archive $zip -DestinationPath $env:TEMP -Force
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item "$env:TEMP\op-reason-windows\*" $dir -Recurse -Force
& "$dir\op-reason.exe" --version
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
  [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')    # for terminals started from now on
}
```

Terminals started afterwards see the new `PATH`; WSL sees it after `wsl --shutdown` (or set `OPREASON_EXE`).

- **`op-reason.exe` is not signed.** Windows may warn before it starts (SmartScreen), and with Smart App Control on it
  may refuse to start it at all, again for each new version. Calls from WSL then fall back to `launcher.ps1` by
  themselves (one line on stderr says so); on Windows use the PowerShell entry below.
- It is called the same way from PowerShell, cmd.exe and other programs, and the arguments reach `op` as given (`--`
  needs no quotes).
- **Without a terminal** (a scheduled task, a double click, a shortcut), Windows 11 24H2 and later (and Windows Server
  2025) give `op-reason.exe` no console window: it asks for none unless it inherits one. A double click opens the
  settings window; what such a start would print goes nowhere, so run `op-reason` in a terminal. Older Windows show a
  console window for a moment.
- **Output in PowerShell**: PowerShell decodes a program's output with `[Console]::OutputEncoding`. When `op`'s output
  is not ASCII, run `[Console]::OutputEncoding = [Text.Encoding]::UTF8` before `$x = op-reason …`. For binary output
  use `op`'s `--out-file`.
- **The log** is `%LOCALAPPDATA%\op-reason\log.jsonl` (the same format as on WSL); `op-reason --log` shows it.
- Calls from WSL and from Windows run one at a time: `op-reason.exe` and the launcher hold the mutex `Local\op-reason`
  while a call runs.
- **Calls from WSL**: `OPREASON_RUNNER=launcher` makes them skip `op-reason.exe` (where Smart App Control blocks it,
  this saves the attempt and its notice on every call); `OPREASON_RUNNER=exe` never falls back.

#### If Smart App Control blocks op-reason.exe: the PowerShell entry

The zip's `powershell-entry` folder holds `op-reason.ps1` (the entry), `op-reason.cmd` (a shim for cmd.exe and other
programs) and copies of `launcher.ps1` and `SKILL.md`. These are scripts for Windows PowerShell, which comes with
Windows, and they ran on a machine with Smart App Control on. They are slower: the window appears one to one and a
half seconds after the call, against a few tens of milliseconds for `op-reason.exe` (measured on GitHub's Windows
runners). Add `<folder>\powershell-entry` to your `PATH` **in front of** `<folder>`: `op-reason` then runs the
PowerShell entry, and WSL still finds `op-reason.exe` and `launcher.ps1` in `<folder>` (calls from WSL need no change).
The entry has a folder of its own because PowerShell prefers `op-reason.ps1` to `op-reason.exe` in one folder.

- **From PowerShell**, `op-reason` runs `op-reason.ps1`. Scripts must be allowed: the default execution policy
  (Restricted) refuses them, and files from the internet carry the Mark of the Web. Either
  `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` and `Unblock-File <folder>\powershell-entry\*.ps1`, or call `op-reason.cmd`
  instead. Shells that already run with `-ExecutionPolicy Bypass` (such as Claude Code's tool shell) can call
  `op-reason.ps1` directly.
- **From cmd.exe and other programs**, call `op-reason.cmd`. cmd.exe re-parses the arguments: an argument without
  spaces that contains `&`, `|`, `<`, `>`, `^` or `%` can break (the problem known as "BatBadBut"). Quote such
  arguments, or call `op-reason.ps1` through PowerShell.
- **`--` in PowerShell**: PowerShell takes an unquoted `--` for itself. Write `'--'` for the child command of
  `op run`: `op-reason --reason "…" run '--' my-command`.
- **Output in PowerShell**: `$x = op-reason …` captures `op`'s output as text (UTF-8). For binary output use
  `op`'s `--out-file`, or `op-reason.cmd` with a redirection in cmd.exe.
- **The log** is the same file. The PowerShell entry has no log viewer:
  `Get-Content $env:LOCALAPPDATA\op-reason\log.jsonl | ConvertFrom-Json | Format-Table time, event, reason`.

### fish completion

```sh
op-reason --completion fish >~/.config/fish/completions/op.fish
op-reason --completion fish op-reason >~/.config/fish/completions/op-reason.fish
```

The completion never runs `op` (fish's own `op` completion runs `op completion fish`, which the wrapper refuses without
a reason). The Nix packages install it already.

## Usage

```sh
op --reason "read the Proxmox API token to create the staging VM" read "op://Private/proxmox/token"
```

- Write the reason as one sentence: what for, and what you read or write. It appears in the window and the log.
- `--reason "<reason>"` and `--reason=<reason>` both work, anywhere before `--` (after `--`, the arguments belong to
  `op`, such as the child command of `op run`).
- `op --version`, `op --help`, `op help <command>` and `op <command> --help` need no reason (no prompt, no window).
- `op` without arguments prints the wrapper's usage.
- **Deny any 1Password CLI approval prompt that comes without the op-reason window.**

![Animation: op read "op://Private/proxmox/token" without --reason is refused at once, before any window or approval prompt: op-reason prints its short usage and "op-reason: There is no reason (--reason). People need it too.", and echo $? shows the exit code 2.](docs/images/readme-3-no-reason.en.svg)

The examples use `op`, the symlink. Called as `op-reason`, the wrapper behaves the same, except that `--help` and
`--version` alone are the wrapper's own. On Windows, type `op-reason`.

### The window

`op-reason.exe` draws the window of a call at the top right of the screen you work on: the reason first, in large
letters; the caller (Claude Code with its process id, or a person), herdr's workspace and pane, the repository with its
branch, and the directory as chips; the command in a monospace box, the program and the file names in color; the
references the call reads ([below](#the-references-a-call-reads)); a band with a small picture of the screen that marks
where 1Password's prompt usually opens (the middle of the screen) and where it waits when it does not (1Password's icon
in the tray at the bottom right, under ^ when hidden); and the red line that tells you to deny prompts that come without
the window. It follows Windows' app mode (Settings → Personalization → Colors), dark or light, also when the mode changes
while the window is open. The PowerShell launcher's window (the fallback) keeps its plain look.

### Wrapper options (first argument only)

| Command | What it does |
| --- | --- |
| `op-reason --help`, `op-reason -h` | the wrapper's usage |
| `op-reason --version`, `op-reason -V` | op-reason's version |
| `op-reason --skill` | a guide for AI agents (a SKILL.md file); refusals point agents to it |
| `op-reason --completion fish [<command>]` | the fish completion for `op` (or `<command>`) |
| `op-reason-log`, `op-reason --log` | the log viewer (below) |
| `op-reason --settings …` | op-reason's settings: `list`, `get`, `set`, `check`, the SSH relay; without more words, the settings window ([Settings](#settings)) |

### The log

```sh
op-reason-log              # calls that started in the last hour
op-reason-log --since 1d   # 30m, 2h, 1d, …
op-reason-log --all        # everything kept
op-reason-log --json       # one JSON object per call, reason and command not cut, every reference read
```

![Animation: op-reason-log prints one line per call: the time, the result (approved, refused (no reason), denied), the seconds, the caller (Claude or Human), the reason and the command. The Caller, Reason and Result columns light up in turn.](docs/images/readme-4-log.en.svg)

The log is `${XDG_STATE_HOME:-~/.local/state}/op-reason/log.jsonl` on WSL and `%LOCALAPPDATA%\op-reason\log.jsonl` on
Windows, where `op-reason --log` takes the same options (renamed to `log.1.jsonl` at 1 MiB). It never holds
`op`'s output or tokens; values of `--session`, `--secret-key` and `name=value` arguments are masked. The record format
is described in [docs/architecture/README.md](docs/architecture/README.md).

### The references a call reads

The window lists the `op://` references the call reads, under the command line: those in the arguments, those in the
template of `op inject` (its `-i` file, or the template on stdin) and those in the `--env-file`s of `op run`. It shows
eight of them and then "and N more"; the log keeps all, and `op-reason-log` adds their number to the command when they
came from a file or stdin.

```text
Command: op inject -i app.env.tpl -o .env
References to read (10, from app.env.tpl):
    op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/password    ← a link
    op://Private/proxmox/token
    …
    and 2 more
```

- A reference whose vault and item are IDs (26 lowercase letters or digits, as `op item get … --format json` and a
  private link show them) is a link: press and release on it, and the 1Password app comes to the front with the item
  open, while the approval prompt stays in front of it and can still be answered. op-reason's window never takes the
  focus. While 1Password is locked, a click changes nothing. References with names or variables (`$APP_ENV`) stay
  plain text.
- The link needs the account ID. At the end of each approved call op-reason already runs `op whoami`; it now reads the
  account ID and the sign-in address from its JSON output and keeps only those two in
  `%LOCALAPPDATA%\op-reason\account.json`. So **the first call has no links** (the window says why), and later calls
  link the references of the account `op` uses by default. With `--account`, the account must be one that whoami
  reported before (by its ID, its sign-in address or the first part of the address). Delete the file to forget the
  accounts.
- From WSL the template on stdin is read by op-reason first (op gets the same bytes); the PowerShell entry (the Smart
  App Control fallback) does not read stdin, so there only the arguments and files are listed.

### AI agents

Agents that have not seen op-reason before should run `op-reason --skill` once: it explains how to call `op` with a
reason and what to do when a call is denied or times out (do not retry; ask the user). To place it where Claude Code
finds it by itself: `op-reason --settings set skill.windows on` on Windows, `set skill.wsl.<distribution> on` for a WSL
distribution ([Settings](#settings)).

### Language

English by default. Click **日本語** (or **English**) at the bottom right of the window to switch: the window changes
at once, without taking the focus from the 1Password prompt, and every later call uses that language for the window,
the notification, the messages and `op-reason-log`, on Windows and on WSL. The choice is kept in
`%LOCALAPPDATA%\op-reason\settings.json`; `op-reason --settings set lang ja` sets it too, and `set lang default` goes back
to `OPREASON_LANG` (on WSL, `op-reason --settings` updates the copy `~/.local/state/op-reason/lang.json`; otherwise the
next call does).

![Animation: op-reason's window opens in English, its default, at the top right of the screen, with the language switch, a pill of English and 日本語, at the right of its last row, and the 1Password approval prompt (drawn as a plain box) opens in the middle of the screen. A click on 日本語 turns the window Japanese at once: 日本語 is now bold on the lighter half of the pill, and English is the half to click. The prompt keeps the focus, and the choice is kept in %LOCALAPPDATA%\op-reason\settings.json. After the approval, the next call opens its window in Japanese.](docs/images/readme-5-language.en.svg)

### Exit codes

| Code | Meaning |
| --- | --- |
| `op`'s code | the call ran |
| 2 | no reason, an empty reason, arguments too long for Windows, or a wrong wrapper option |
| 75 | another call (WSL or Windows) is still waiting for its approval (the lock wait timed out) |
| 127 | `op.exe`, `op-reason.exe` (and its fallback), `launcher.ps1`, `powershell.exe` or `wslpath` was not found, or could not be started, or `wslpath -w` could not convert a path |
| 1 | `op-reason.exe` or the launcher could not start `op.exe` |
| 129, 130, 143 | stopped by SIGHUP, SIGINT, SIGTERM (WSL); on Windows, 130 for Ctrl+C and 129 when the console of `op-reason.exe` closes |

### Environment variables

| Variable | Meaning |
| --- | --- |
| `OPREASON_LANG` | `en` or `ja`: language of messages, the window and the log viewer while none was chosen in the window ([Language](#language)). Default: English (`LANG` and the Windows display language do not count) |
| `OPREASON_LOCK_WAIT` | seconds to wait for another call's approval (default 90, below the 120-second default timeout of Claude Code's Bash tool) |
| `OPREASON_OP_EXE` | path of `op.exe` (default: `op.exe` in `PATH`) |
| `OPREASON_EXE` | WSL: path of `op-reason.exe` (default: `op-reason.exe` in `PATH`); `launcher.ps1` is looked for in its folder |
| `OPREASON_RUNNER` | WSL: `auto` (default: `op-reason.exe`, and `launcher.ps1` when the exe did not start the call), `exe` (never the launcher) or `launcher` (never the exe) |
| `OPREASON_POWERSHELL` | WSL: the `powershell.exe` command for the launcher (default `powershell.exe`) |
| `OP_SERVICE_ACCOUNT_TOKEN`, `OP_CONNECT_HOST` + `OP_CONNECT_TOKEN` | service account or Connect: `op` runs without the desktop app (no window, no prompt) and gets the token; desktop calls never get these variables |
| `HERDR_ENV`, `HERDR_WORKSPACE_ID`, `HERDR_PANE_ID` | inside [herdr](https://herdr.dev/), the window shows the workspace and pane (optional) |
| `OPREASON_SSH_REASON` | the reason of an ssh call through the [SSH relay](#ssh-keys-the-relay) (Windows clients, and `op-reason-ssh` on WSL) |
| `OPREASON_SSH_EXE` | WSL: the `ssh.exe` that `op-reason-ssh` runs (default: `ssh.exe` in `PATH`) |
| `OPREASON_SSH_CALL` | set by `op-reason-ssh` for the relay (the facts of a call from WSL); not for you to set |

The prefix is `OPREASON_`, not `OP_`: the 1Password CLI reads many `OP_…` variables of its own.

## SSH keys: the relay

`op-reason.exe --ssh-agent` stands in front of the 1Password SSH agent: it runs in the background, serves the SSH agent
protocol on its own pipe, `\\.\pipe\op-reason-ssh-agent`, and passes every request on, unchanged, to the 1Password agent
(`\\.\pipe\openssh-ssh-agent`). Only signatures wait for a decision; listing the keys and everything else pass at once.
Design: [docs/superpowers/specs/2026-11-17-ssh-relay-design.md](docs/superpowers/specs/2026-11-17-ssh-relay-design.md).

Why: 1Password's SSH prompt names the application and the key, not the caller, the destination or the reason; for ssh
run in a terminal, the application is the terminal program, from Windows and WSL alike. With its setting "Ask approval
for each new" at "application and terminal session", one approval lets every later signature from that terminal
session through until 1Password locks, and at "application" every later signature from that terminal program: after
you approved once, an agent in the same terminal can use the key without a prompt. At "request" it asks for every
signature, which ansible on WSL (new connections for every task and host) turns into dozens of prompts per run.

### Two modes

| Mode | Where you decide | 1Password's setting |
| --- | --- | --- |
| `per-call` (default) | op-reason's window, once per call: **Allow this call**, **Deny**, and for `ssh`, `git`, `scp`, `sftp`, `rsync` or `ansible` typed in a terminal, **Allow this terminal for 60 minutes**. 1Password approves the relay once, until it locks | "application and terminal session" (recommended); "application" works too, but then 1Password no longer asks about clients that connect to it directly from the terminal program the relay runs in ([What it does not stop](#what-it-does-not-stop)) |
| `show-only` | 1Password's prompt, for every signature; op-reason's window shows who and why next to it | "request" |

A **call** is its root process, the key and the reason. The root is Claude Code's tool call, the command you typed in a
terminal (`git push`, `ansible-playbook`, `ssh`), or the program that runs ssh itself; from WSL, `op-reason-ssh` finds
it on Linux. Once allowed, the call's later signatures pass without a window, each shown in a toast at the top left
([Windows, toasts and notifications](#windows-toasts-and-notifications); ansible's forks wait for one answer); a denied
call fails at once. An answer lasts until the root process ends (a root on WSL only times out), until 60 minutes
have passed (`--time-limit`), or until the relay stops, and it is kept in memory only. The terminal answer covers the
ssh-family commands typed later in that shell, not programs that live on (a terminal program, another AI agent's
command-line tool). Requests forwarded back from a remote host (`ssh -A`) are each a call of their own: the relay
tells them by `session-bind@openssh.com`, which OpenSSH sends from 8.9 on; with an older `ssh.exe` every signature asks.

The window puts the reason first (or "no reason given" in its place), then who asks as chips (Claude Code, a person, a
program; for WSL calls also the distribution and herdr's workspace and pane; the repository, the branch and the
directory), the root command in a monospace box (`git push origin fix/login`: the wrappers of a shell and of Claude
Code's Bash tool taken off when they match exactly; the root process and the rest of its line in small type under it),
what the signature is for (logging in as which user, or an SSHSIG namespace such as `git`), the destination with its
host key and the key's name in 1Password, and the ssh command in small type. A bar shows the seconds left to answer,
next to how long an answer lasts ("Allowing covers this git push until it ends (at most 60 minutes)"), and a band says
that 1Password's prompt comes after Allow, usually in the middle of the screen, otherwise from the tray. Only a click
answers it (a press and a release on a button): it opens without taking the focus and has no default button, so a key
typed elsewhere never answers it. **After a click on Allow this call or Allow this terminal, it comes to the front**
until 1Password has answered ("SSH key: 1Password is asking"), so that 1Password shows its prompt at once
([why](#starting-it)); then the window that had the front gets it back, unless another window took it meanwhile. It
stays behind when the window in front belongs to the program the relay was started from (a terminal program, which
1Password sees in front already), and **Deny** never takes the front. No answer within 60 seconds fails that signature
without remembering anything.

> **The rule changes in per-call mode.** Everywhere else in op-reason the decision is made in 1Password's prompt, and
> op-reason's window only shows facts. In per-call mode op-reason's window is where you decide, because 1Password cannot
> tell calls apart and asking it for every signature is not usable. show-only keeps the old rule.

### Windows, toasts and notifications

op-reason shows three things on the screen, each in its own place
([design](docs/superpowers/specs/2026-11-18-pass-toasts-design.md)):

| What | Where | Gone |
| --- | --- | --- |
| op-reason's window: it asks (per-call), or shows who and why while 1Password asks | the top right of the screen of the foreground window | when the request is answered |
| A toast: a signature that passed without a window (per-call) | the top left of that screen | four seconds after the call's last signature |
| The Windows notification of a window | the bottom right (Windows' notifications) | with its window, unless the window timed out unanswered |

- **Toasts.** A signature that an earlier answer lets through (**Allow this call**, **Allow this terminal**) shows a
  card in the colors of op-reason's window (dark or light, as Windows' app mode) for four seconds, then fades: `Passed ·
  <caller>`, the ssh command and its destination, why it passed (allowed for this call, allowed for this terminal) and
  the reason. The signatures of one call share one card, which counts them (×N), shows the last destination and starts
  its four seconds again; each other call gets its own card at the top, three at most: a fourth pushes the oldest out at
  once, and `+N` under the third card counts the cards pushed out until four seconds after their last signature. A card
  is drawn at 90% opacity, so that the text behind it shows through faintly. Toasts never take the focus, let clicks
  through, make no sound and stay out of the notification center. With Windows' animation effects off they neither slide
  nor fade.
- **Windows** open on the screen you work on (that of the foreground window; the primary screen when there is none)
  and stay there. The windows of `op` calls do too, when `op-reason.exe` runs them; the PowerShell launcher's window
  stays at the top right of the primary screen, and its notifications stay. They open without taking the focus; only
  the relay's window comes to the front, after a click on Allow, until 1Password has answered.
- **Notifications** go with their window, so they do not pile up in the notification center; a per-call window that
  timed out leaves its notification as the sign of a request that went unanswered. The one-time notices (1Password
  did not ask, 1Password did not answer, the relay has no parent process) stay.

![Animation of a Windows desktop. A signature that passes without a window shows a small dark card in the colors of op-reason's window, a toast, at the top left, at 90% opacity so that the text behind shows through faintly: Passed · Human, the ssh command and its destination, allowed for this terminal · no reason given; it fades after four seconds. An ansible run signs eight times in one call: one card counts them (×2 … ×8), shows the last host and lights up at each. Three other calls each put a card on top and push the others down; a fourth and a fifth push the oldest out at once, and +N under the third card counts the cards pushed out (+1, +2) until four seconds after their last signature; four seconds after its last signature each card fades. Then a new call asks in op-reason's window at the top right (the root command in a box, the seconds left to answer counting down), with its Windows notification at the bottom right, while another toast comes at the top left; a click on Allow this call closes the window and its notification together, and the call's later signatures come as one toast at the top left (×3).](docs/images/readme-6-relay-screens.en.svg)

🌐 [HTML version: the relay's windows, toasts and notifications](docs/html/relay-screens.html)

### Starting it

```powershell
op-reason.exe --ssh-agent                      # per-call; until Ctrl+C or logoff
op-reason.exe --ssh-agent --mode show-only
op-reason.exe --ssh-agent --status             # exit 0 (and its process id) when a relay answers on the pipe
```

- **Start it at logon from a scheduled task** (hidden), with a trigger that also repeats every minute: the task's
  "restart on failure" did not start a relay that had been stopped again. `op-reason --settings set relay.autostart on`
  makes such a task, op-reason's own, and starts the relay ([Settings](#settings)); the points below are its shape. Keep the task's rule "If the task is already
  running" at **Do not start a new instance** (the default; `-MultipleInstances IgnoreNew`): with "Stop the existing
  instance" the relay would be stopped and started again every minute, and a relay started while another one runs
  exits at once (exit code 1), which the task would show as its last result. On Windows 11 24H2 and later (and Windows
  Server 2025), a relay that a task starts shows no console window: `op-reason.exe` asks Windows for no console unless
  it inherits one (a terminal's). Older Windows make it one, which shows for a moment until the relay lets go of it.
- **1Password shows its prompt only while it sees the application that asks in front.** Otherwise it keeps the prompt
  behind its icon at the right of the taskbar (the icon gets a dot, and its menu has **SSH request waiting**) and
  closes the request after about 60 seconds. For a relay started in a terminal program, that application is the
  terminal program; for a relay started from a scheduled task, or whose parent process has ended, it is the relay
  itself, op-reason. So in per-call mode op-reason's window comes to the front at the click on Allow, and 1Password's
  prompt comes at once, naming op-reason (1Password approves each run of the relay once, until it locks). In show-only
  mode, and when 1Password asks about a request that an earlier answer let through (to unlock it, for example), nobody
  clicked: open the prompt from the icon, as the last line of op-reason's window "1Password is asking" says
  ([design](docs/superpowers/specs/2026-11-22-front-after-allow-design.md)). The relay says in a toast and a record
  when its parent is missing or ends.
- One relay per logon session. Options: `--mode per-call|show-only`, `--pipe <name>`, `--upstream <name>`,
  `--time-limit <n>[s|m|h]` (60m, at most 24h), `--answer-wait <n>[s|m]` (how long a window waits for a click, 60s, at
  most 60m). Without them, the relay takes these values from `settings.json` ([Settings](#settings)). op-reason's task
  adds `--task <its name>`, which the relay writes into `relay.json` so that `op-reason --settings` knows it for its own.
- While its window is open, the relay holds op-reason's lock, so that its windows and 1Password's prompts come in pairs
  with those of `op` calls.

### Pointing ssh at it

- **Windows**: `SSH_AUTH_SOCK` set to `\\.\pipe\op-reason-ssh-agent` for Windows OpenSSH, or in `~/.ssh/config`
  `IdentityAgent \\\\.\pipe\op-reason-ssh-agent` (OpenSSH reads `\\` in its configuration and in `-o` as one backslash, so
  the leading pair is written twice). The reason goes in `OPREASON_SSH_REASON`:
  `$env:OPREASON_SSH_REASON = "push the login fix"; git push`. Git for Windows runs its own ssh, which cannot reach the
  pipe: set `GIT_SSH` (or `core.sshCommand`) to Windows OpenSSH's `ssh.exe`, such as
  `C:\Windows\System32\OpenSSH\ssh.exe` or `C:\Program Files\OpenSSH\ssh.exe`.
- **WSL**: run `op-reason-ssh` (the WSL executable under that name) instead of `ssh.exe`, for example
  `GIT_SSH_COMMAND=op-reason-ssh` or ansible's `ansible_ssh_executable=op-reason-ssh`, with the reason in
  `OPREASON_SSH_REASON`. It finds the root of the call on Linux, collects the caller as `op` does (Claude Code, herdr,
  the repository), the distribution and the reason, hands them to the relay in `OPREASON_SSH_CALL` (through `WSLENV`),
  and runs `ssh.exe` with your arguments, stdin, stdout and stderr unchanged (`ssh.exe` reads the Windows
  `~/.ssh/config`, so the `IdentityAgent` line above serves WSL too). When it cannot collect, it still runs `ssh.exe`
  (the relay then asks for each signature).

### The records

Every signature is recorded in `%LOCALAPPDATA%\op-reason\ssh-log.jsonl` (rotated at 4 MiB, two old files), never with
the data signed or the signature. `op-reason.exe --log` shows them among the calls of `op`, one line each:

```text
Time      Result       Secs  Caller           Reason              Command
09:00:12  SSH allowed  2.5   Claude deploy …  push the login fix  ssh.exe git@github.com "git-receive-pack 'me/r.git'"
09:00:13  SSH passed   0.1   Claude deploy …  push the login fix  ssh.exe git@github.com "git-receive-pack 'me/r.git'"
```

### What it does not stop

- A program of your own account that wants to get around it: it can fake `OPREASON_SSH_CALL`, run under an allowed
  root, or send clicks. As for `op`, op-reason shows facts, not proofs.
- A client that talks to 1Password's pipe directly. 1Password asks only when it has not approved the request's
  application since it locked (at "application and terminal session", the request's pair of application and terminal
  session), and the application it sees is not the client but the terminal program the client runs in. So at
  "application", once the relay is approved, a client that connects directly from the terminal program the relay runs
  in passes without a prompt; at "application and terminal session", a client in another terminal session is asked.
  **A 1Password SSH prompt without op-reason's window is the sign.**
- Other AI agents' command-line tools are seen as a person (typed in a terminal) or a program: only Claude Code's tool
  calls are told apart. Their requests still wait for a click, and the terminal answer does not cover them.
- WSL calls are told by the parent of `ssh.exe`: `wsl.exe`, `wslhost.exe` or `C:\Windows\System32\bash.exe`. A WSL
  started through a distribution's own launcher (such as `ubuntu.exe`) may hand its `ssh.exe` another parent; the relay
  then treats it as a Windows program (to be checked by hand, [docs/manual-verification.md](docs/manual-verification.md)).
- Where Smart App Control blocks `op-reason.exe`, there is no relay (there is no PowerShell fallback for it). Git Bash's
  `ssh` (MSYS2) does not use the Windows pipe, and git commit signing with `op-ssh-sign` talks to the 1Password app
  directly (seen with `op-ssh-sign-wsl.exe`): it stays outside the relay.

## Settings

`op-reason --settings` reads and changes op-reason's settings: the values that op-reason reads itself, kept in
`%LOCALAPPDATA%\op-reason\settings.json`, and what Windows, Task Scheduler and Claude Code read elsewhere (the relay's
scheduled task, the running relay, the skill files). Every setting has the same three operations: `get` reads the value
now, `set` makes it so (running it again changes nothing), and `check` compares a wanted value with the value now.
Without more words it opens [the settings window](#the-window-the-start-menu-and-the-icon), which does the same with
buttons. Design: [docs/superpowers/specs/2026-11-23-settings-design.md](docs/superpowers/specs/2026-11-23-settings-design.md).

| Setting | Values (default) | Kept in |
| --- | --- | --- |
| `lang` | `en`, `ja`, `default` (`default`: `OPREASON_LANG`, else English) | `settings.json` |
| `relay.mode` | `per-call`, `show-only` (`per-call`) | `settings.json`, read by the relay when it starts; its options win |
| `relay.time-limit` | `<n>[s\|m\|h]`, a bare number is minutes, at most `24h` (`60m`) | `settings.json` |
| `relay.answer-wait` | `<n>[s\|m]`, a bare number is seconds, at most `60m` (`60s`) | `settings.json` |
| `relay.autostart` | `on`, `off` (`off`) | op-reason's scheduled task `\op-reason-ssh-relay-<your SID>` |
| `relay.running` | `on`, `off` | the relay that answers on `\\.\pipe\op-reason-ssh-agent` |
| `relay.tray-icon` | `on`, `off` (`on`) | `settings.json`; the relay's icon in the notification area follows it at once |
| `start-menu` | `on`, `off` (`off`) | `op-reason.lnk` in your Start menu's Programs folder (`%APPDATA%\Microsoft\Windows\Start Menu\Programs`) |
| `skill.windows` | `on`, `off` (`off`) | `%USERPROFILE%\.claude\skills\op-reason\SKILL.md` (Claude Code on Windows) |
| `skill.wsl.<distribution>` | `on`, `off` (`off`) | `~/.claude/skills/op-reason/SKILL.md` in that WSL distribution |
| `skill.folder.<folder>` | `on`, `off` (`off`) | `<folder>\op-reason\SKILL.md`, for another agent or a project |

```powershell
op-reason --settings list                          # every setting: its value now, its default, where it is kept
op-reason --settings get relay.mode                # per-call
op-reason --settings set relay.autostart on        # make op-reason's task and start the relay
op-reason --settings set relay.mode=show-only relay.time-limit=30m
op-reason --settings check relay.autostart=on relay.running=on --json
op-reason --settings relay stop                    # stopped until relay start, also after a logon
op-reason --settings set skill.windows on          # place the guide where Claude Code finds it
```

- **Exit codes**: `check` exits 0 when every value is as wanted, 1 when one differs and 2 when one could not be read
  (or the command was wrong); `set` and `relay …` exit 1 when something could not be done, each line saying why, and
  2 for a wrong command (nothing done). `list`, `get` and `check` print JSON with `--json`: `state` is a code (`ok`,
  `relay-differs`, `task-differs`, `task-off`, `other-relay`, `old`, `foreign`, `link`, …) with its facts in `detail`.
- **op-reason's scheduled task** has the shape that [Starting it](#starting-it) describes: the relay at logon and again
  within a minute when it stops, your own rights, hidden (Task Scheduler shows it with **View → Show Hidden Tasks**).
  The relay reads its mode and times from `settings.json` when it starts; `set` starts op-reason's relay again when its
  values change (1Password then asks once more, and remembered answers are forgotten). **`relay stop`** turns the task
  off and ends the relay: it stays stopped, also after the next logon, until `relay start`. A stop or a restart takes
  op-reason's lock first: while the relay's window is open or a call waits for 1Password, it is refused, and the
  requests that are with 1Password get up to 25 seconds to finish.
- A relay that another scheduled task or a terminal started is shown, never stopped. While one runs,
  `set relay.autostart on` makes op-reason's task turned off: `relay start` once the other one has stopped.
- **The skill** is a copy of `op-reason --skill` with a mark after its front matter (`<!-- placed by op-reason <version>:
  … -->`): `set … on` places it or updates an older copy, `off` removes it, and neither touches a `SKILL.md` without the
  mark nor writes through a junction or a symbolic link (home-manager may make `~/.claude/skills` one). A WSL
  distribution is reached through `wsl.exe` (naming one that does not run starts it; `list` never does).
- From WSL, `op-reason --settings` runs `op-reason.exe --settings` (a folder given as a WSL path is passed as its
  Windows path). The PowerShell entry does not change settings.
- Every change is recorded in the log with its caller (`op-reason --log` shows it). Agents are told to change settings
  only when the user asks; as for `op`, a program of your account can still change them.

### The window, the Start menu and the icon

`op-reason --settings` without more words, and a double click on `op-reason.exe`, open the settings window and return
at once (there is one window per logon; starting it again brings it to the front). It speaks the language of `lang`
and changes it at once:

- **SSH relay**: a line that says whether the relay runs, its process, its mode and who started it. **Start**, **Stop**
  and **Restart** are greyed when op-reason may not act, for example on a relay that another task started. The check
  boxes are `relay.autostart` and `relay.tray-icon`. The mode and the times are written when they change and used when
  the relay starts again: **Apply** restarts op-reason's relay with them, after a question (1Password asks once more,
  and remembered answers are forgotten).
- **Skill for AI agents**: the places and their states. **Place or update**, **Remove**, **Check** and **Add a
  folder…**. A WSL distribution that does not run is not looked at when the window opens; an action on its row starts
  it.
- **Start menu and records**: `start-menu`, the records folder (**Open**) and the accounts remembered for links
  (**Forget**).
- Every action goes through the same layer as the command line (`set`; the values kept in `settings.json` are
  written without starting the relay again), on a thread of its own, and is recorded with `from: window`.

`set start-menu on` puts **op-reason** in the Start menu, so that Windows' search finds the window: `op-reason.lnk`
starts this `op-reason.exe` with `--settings` (on Windows before 11 24H2, its console shows for a moment in the
taskbar). After the exe moves,
`set start-menu on` makes the shortcut again. A shortcut of that name that is not op-reason's is left alone.

While the relay runs, it shows a key in the notification area (Windows 11 puts a new icon behind `^`; drag it onto the
taskbar to keep it in sight). A click opens the settings window; the menu shows the relay's line and opens the
settings or the records folder, restarts or stops op-reason's relay (greyed for a relay that op-reason's task did not
start), and hides the icon (`relay.tray-icon off`). The menu's actions run `op-reason.exe --settings …` in a process
of their own and are recorded with `from: tray`.

## Things to know

- **`signout --all` closes every CLI session**, not only the one of this call: also an `op` that you typed by hand in
  another terminal tab at the same time. op-reason calls run one at a time, so they do not close each other's
  sessions (two calls connected by a pipe are handled: the sender leaves the sessions alone).
- **One call at a time.** While one call waits for its approval, the next waits up to `OPREASON_LOCK_WAIT` seconds and
  then gives up with exit 75. `op … | op …` works: the receiver reads the sender's output before waiting.
- **Long-running `op run`.** The lock and the window stay until `op` exits. Avoid `op run` for servers; resolve the
  values and let `op` exit.
- **A tool that kills the call.** If a tool stops `op-reason` while the prompt is open (for example a command
  timeout), the window may disappear while the prompt stays: deny it. If you approved it, lock 1Password: the approval
  stays bound to a process that has exited until 1Password's 10-minute limit. When `op-reason.exe` itself is killed,
  `op.exe` and its prompt end with it (a Job Object); how a stop from WSL reaches `op-reason.exe` is still to be
  checked by hand ([docs/manual-verification.md](docs/manual-verification.md)).
- **Shared approvals.** Before and after each call op-reason runs `op.exe whoami` directly from WSL. If that succeeds,
  an approval usable from every terminal exists (someone approved a prompt without the window, or a tool ran `op.exe`
  directly); op-reason signs it out, logs it and shows a notification.
- **Ctrl+C on Windows**: `op-reason.exe` stops `op`, signs out, logs `interrupted` and exits with 130. Through the
  PowerShell entry, a call stopped by Ctrl+C is logged as `interrupted` (exit 130) when PowerShell lets the entry clean
  up; the exit code of the PowerShell process itself is PowerShell's.
- **pipes on Windows**: one op-reason call piped into another (`op-reason … | op-reason …`) is not detected on Windows;
  the second call can wait for the lock until `OPREASON_LOCK_WAIT` runs out. Store the first output in a variable.
- The WSL watchdog runs `op.exe signout --all` outside the Windows lock; when it finds a shared approval it can close
  the session of a Windows call that runs at that moment (rare).
- Not verified yet: whether plain `op signout` (without `--all`) closes only one process's session.
- **A link while the approval prompt is up** opens the item in the 1Password app, which comes to the front; the
  prompt itself is the app's. How the app shows both is still to be checked by hand
  ([docs/manual-verification.md](docs/manual-verification.md)).

## Development

```sh
nix develop path:./nix                       # Rust, pwsh, fish, jq, shellcheck, shfmt, python3
cargo test --workspace                       # unit tests and the call table
cargo build && bash tests/run-all.sh         # black-box tests with fake op-reason.exe, powershell.exe and op.exe
python3 docs/images/make_readme_svgs.py      # the animations of the READMEs (docs/images/*.svg)
# op-reason.exe is a Cargo workspace of its own: in crates/op-reason-win, `cargo build -p fake-op -p fake-ssh` and then
# `cargo test --workspace` on Windows (on Linux, the portable parts only)
# On Windows (Windows PowerShell 5.1): tests\windows\launcher.test.ps1, entry.test.ps1, entry-plan.test.ps1
```

- The animations of the READMEs are made by `docs/images/make_readme_svgs.py` (Python, standard library only). To
  change a text or the layout, change the script, not the SVGs, and run it again in the development shell. Their
  texts are copies of op-reason's (the window, the refusal, the log table, the relay's window and toasts): `--check`,
  which CI runs, fails when the SVGs differ from the script's output, or when a copied text or time is no longer in the
  Rust sources or in the HTML page of the relay's toasts (`docs/html/relay-screens.html`).
- The checks with the real 1Password app are listed in [docs/manual-verification.md](docs/manual-verification.md).
- The design: [docs/superpowers/specs/2026-11-15-op-reason-design.md](docs/superpowers/specs/2026-11-15-op-reason-design.md);
  `op-reason.exe`: [docs/superpowers/specs/2026-11-15-windows-exe-design.md](docs/superpowers/specs/2026-11-15-windows-exe-design.md);
  calls from WSL through it: [docs/superpowers/specs/2026-11-15-wsl-through-exe-design.md](docs/superpowers/specs/2026-11-15-wsl-through-exe-design.md);
  the window's language: [docs/superpowers/specs/2026-11-15-language-toggle-design.md](docs/superpowers/specs/2026-11-15-language-toggle-design.md);
  the SSH relay: [docs/superpowers/specs/2026-11-17-ssh-relay-design.md](docs/superpowers/specs/2026-11-17-ssh-relay-design.md);
  its toasts, the screen of the windows and the notifications:
  [docs/superpowers/specs/2026-11-18-pass-toasts-design.md](docs/superpowers/specs/2026-11-18-pass-toasts-design.md);
  its window in front after Allow, and where a hidden prompt waits:
  [docs/superpowers/specs/2026-11-22-front-after-allow-design.md](docs/superpowers/specs/2026-11-22-front-after-allow-design.md);
  the settings: [docs/superpowers/specs/2026-11-23-settings-design.md](docs/superpowers/specs/2026-11-23-settings-design.md).
- What changed in each release: [CHANGELOG.md](CHANGELOG.md).
- Where to read what: [docs/README.md](docs/README.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
