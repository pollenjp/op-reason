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

It was written for AI coding agents (such as Claude Code) that run `op` on your behalf: you see what each request is
for before you approve it, one approval never covers later requests, and you can review every call afterwards.

## Why a wrapper

- **One approval leaks to later calls.** The 1Password desktop app binds a CLI approval to the process that started
  `op.exe`. From WSL every `op.exe` is a child of the same `wslhost` that all terminals share, so a single approval
  reaches later calls and other terminals. op-reason starts a fresh `powershell.exe` for every call, starts `op.exe`
  from it, and signs out at the end.
- **The prompt does not say who asks.** 1Password's prompt names only the terminal ("WindowsTerminal", or "wslhost"
  from WSL), whatever process asked. op-reason's window shows the caller (a person or Claude Code with its process id),
  the repository, branch and directory, the reason and the command.
- **Nothing records what agents did.** op-reason logs every call, refusal and finding as JSON Lines.

## How it works

![op-reason components](docs/architecture/plantuml/out/01_overview.svg)

For every call the entry (the WSL executable, or `op-reason.ps1` on Windows) checks the reason, collects the caller,
and starts a fresh `powershell.exe` that runs the launcher (a PowerShell script; the WSL executable embeds it). The
launcher takes a lock shared by WSL and Windows (one approval prompt at a time), shows the window at the top right of
the screen, starts `op.exe` with your stdin, stdout and stderr, and when `op` exits runs `op.exe whoami` and, if a
session is open, `op.exe signout --all`. Details: [docs/architecture/README.md](docs/architecture/README.md).

## Requirements

- For the WSL side: WSL 2 with Windows interop (the Windows `PATH` is part of the WSL `PATH`, so `op.exe`, `powershell.exe` and `wslpath`
  are found)
- On Windows: the 1Password desktop app with **Settings → Developer → Integrate with 1Password CLI** turned on, and the
  1Password CLI (`op.exe`, for example `winget install AgileBits.1Password.CLI`)
- Windows PowerShell 5.1 (part of Windows)

## Install

### From a release

Releases start with v0.1.0. Each release has static Linux executables for x86_64 and aarch64 with SHA-256 files.

```sh
arch=$(uname -m)   # x86_64 or aarch64
base=https://github.com/pollenjp/op-reason/releases/latest/download
curl -fLO "${base}/op-reason-${arch}-linux"
curl -fLO "${base}/op-reason-${arch}-linux.sha256"
sha256sum -c "op-reason-${arch}-linux.sha256"
install -m 755 "op-reason-${arch}-linux" ~/.local/bin/op-reason
ln -sf op-reason ~/.local/bin/op-reason-log
ln -sf op-reason ~/.local/bin/op     # optional: make `op` the wrapper (~/.local/bin must come first in PATH)
```

### With Nix

```sh
nix profile install github:pollenjp/op-reason      # op-reason, op-reason-log
nix profile install github:pollenjp/op-reason#op   # the same, plus `op`
```

The flake also works as an input: `packages.<system>.default` and `packages.<system>.op`.

### On Windows

Each release has `op-reason-windows.zip` (with a `.sha256` file): `op-reason.ps1` (the entry), `op-reason.cmd` (a shim
for cmd.exe and other programs), `launcher.ps1` and `SKILL.md`. Extract it, keep the files together, and put the folder
in your `PATH`.

- **From PowerShell**, `op-reason` runs `op-reason.ps1`. Scripts must be allowed: the default execution policy
  (Restricted) refuses them, and files from the internet carry the Mark of the Web. Either
  `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` and `Unblock-File <folder>\*.ps1`, or call `op-reason.cmd`
  instead. Shells that already run with `-ExecutionPolicy Bypass` (such as Claude Code's tool shell) can call
  `op-reason.ps1` directly.
- **From cmd.exe and other programs**, call `op-reason.cmd`. cmd.exe re-parses the arguments: an argument without
  spaces that contains `&`, `|`, `<`, `>`, `^` or `%` can break (the problem known as "BatBadBut"). Quote such
  arguments, or call `op-reason.ps1` through PowerShell.
- **`--` in PowerShell**: PowerShell takes an unquoted `--` for itself. Write `'--'` for the child command of
  `op run`: `op-reason --reason "…" run '--' my-command`.
- **Output in PowerShell**: `$x = op-reason …` captures `op`'s output as text (UTF-8). For binary output use
  `op`'s `--out-file`, or `op-reason.cmd` with a redirection in cmd.exe.
- **The log** is `%LOCALAPPDATA%\op-reason\log.jsonl` (the same format as on WSL). There is no log viewer on Windows
  yet: `Get-Content $env:LOCALAPPDATA\op-reason\log.jsonl | ConvertFrom-Json | Format-Table time, event, reason`.
- Calls from WSL and from Windows run one at a time: the launcher holds the mutex `Local\op-reason` while a call
  runs.

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

The examples use `op`, the symlink. Called as `op-reason`, the wrapper behaves the same, except that `--help` and
`--version` alone are the wrapper's own.

### Wrapper options (first argument only)

| Command | What it does |
| --- | --- |
| `op-reason --help`, `op-reason -h` | the wrapper's usage |
| `op-reason --version`, `op-reason -V` | op-reason's version |
| `op-reason --skill` | a guide for AI agents (a SKILL.md file); refusals point agents to it |
| `op-reason --completion fish [<command>]` | the fish completion for `op` (or `<command>`) |
| `op-reason-log`, `op-reason --log` | the log viewer (below) |

### The log

```sh
op-reason-log              # calls that started in the last hour
op-reason-log --since 1d   # 30m, 2h, 1d, …
op-reason-log --all        # everything kept
op-reason-log --json       # one JSON object per call, reason and command not cut
```

The log is `${XDG_STATE_HOME:-~/.local/state}/op-reason/log.jsonl` (renamed to `log.1.jsonl` at 1 MiB). It never holds
`op`'s output or tokens; values of `--session`, `--secret-key` and `name=value` arguments are masked. The record format
is described in [docs/architecture/README.md](docs/architecture/README.md).

### AI agents

Agents that have not seen op-reason before should run `op-reason --skill` once: it explains how to call `op` with a
reason and what to do when a call is denied or times out (do not retry; ask the user).

### Exit codes

| Code | Meaning |
| --- | --- |
| `op`'s code | the call ran |
| 2 | no reason, an empty reason, arguments too long for Windows, or a wrong wrapper option |
| 75 | another call (WSL or Windows) is still waiting for its approval (the lock wait timed out) |
| 127 | `op.exe`, `powershell.exe` or `wslpath` was not found |
| 1 | the launcher could not start `op.exe` |
| 129, 130, 143 | stopped by SIGHUP, SIGINT, SIGTERM (WSL); Windows logs 130 for Ctrl+C |

### Environment variables

| Variable | Meaning |
| --- | --- |
| `OPREASON_LANG` | `en` or `ja`: language of messages, the window and the log viewer. Default: from `LC_ALL`, `LC_MESSAGES`, `LANG` (`ja…` → Japanese), otherwise English |
| `OPREASON_LOCK_WAIT` | seconds to wait for another call's approval (default 90, below the 120-second default timeout of Claude Code's Bash tool) |
| `OPREASON_OP_EXE` | path of `op.exe` (default: `op.exe` in `PATH`) |
| `OPREASON_POWERSHELL` | the `powershell.exe` command (default `powershell.exe`) |
| `OP_SERVICE_ACCOUNT_TOKEN`, `OP_CONNECT_HOST` + `OP_CONNECT_TOKEN` | service account or Connect: `op` runs without the desktop app (no window, no prompt) and gets the token; desktop calls never get these variables |
| `HERDR_ENV`, `HERDR_WORKSPACE_ID`, `HERDR_PANE_ID` | inside [herdr](https://herdr.dev/), the window shows the workspace and pane (optional) |

The prefix is `OPREASON_`, not `OP_`: the 1Password CLI reads many `OP_…` variables of its own.

## Things to know

- **`signout --all` closes every CLI session**, not only the one of this call: also an `op` that you typed by hand in
  another terminal tab at the same time. op-reason calls run one at a time, so they do not close each other's
  sessions (two calls connected by a pipe are handled: the sender leaves the sessions alone).
- **One call at a time.** While one call waits for its approval, the next waits up to `OPREASON_LOCK_WAIT` seconds and
  then gives up with exit 75. `op … | op …` works: the receiver reads the sender's output before waiting.
- **Long-running `op run`.** The lock and the window stay until `op` exits. Avoid `op run` for servers; resolve the
  values and let `op` exit.
- **A tool that kills the call.** If a tool stops `op-reason` while the prompt is open (for example a command
  timeout), the window disappears and the prompt stays: deny it. If you approved it, lock 1Password: the approval
  stays bound to a process that has exited until 1Password's 10-minute limit.
- **Shared approvals.** Before and after each call op-reason runs `op.exe whoami` directly from WSL. If that succeeds,
  an approval usable from every terminal exists (someone approved a prompt without the window, or a tool ran `op.exe`
  directly); op-reason signs it out, logs it and shows a notification.
- **Ctrl+C on Windows**: a call stopped by Ctrl+C is logged as `interrupted` (exit 130) when PowerShell lets the
  entry clean up; the exit code of the PowerShell process itself is PowerShell's.
- **pipes on Windows**: one op-reason call piped into another (`op-reason … | op-reason …`) is not detected on Windows;
  the second call can wait for the lock until `OPREASON_LOCK_WAIT` runs out. Store the first output in a variable.
- The WSL watchdog runs `op.exe signout --all` outside the Windows lock; when it finds a shared approval it can close
  the session of a Windows call that runs at that moment (rare).
- Not verified yet: whether plain `op signout` (without `--all`) closes only one process's session.

## Development

```sh
nix develop path:./nix                       # Rust, pwsh, fish, jq, shellcheck, shfmt
cargo test --workspace                       # unit tests and the call table
cargo build && bash tests/run-all.sh         # black-box tests with fake powershell.exe and op.exe
# On Windows (Windows PowerShell 5.1): tests\windows\launcher.test.ps1, entry.test.ps1, entry-plan.test.ps1
```

- The checks with the real 1Password app are listed in [docs/manual-verification.md](docs/manual-verification.md).
- The design: [docs/superpowers/specs/2026-11-15-op-reason-design.md](docs/superpowers/specs/2026-11-15-op-reason-design.md).
- Where to read what: [docs/README.md](docs/README.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you,
as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
