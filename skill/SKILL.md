---
name: op-reason
description: >-
  Use when running the 1Password CLI (`op`) on a machine where `op` is op-reason, a wrapper that requires a reason
  for every call (WSL or Windows), or when an `op` call was refused for a missing reason or ended with an op-reason
  exit code (2, 75, 127). Also when running ssh, git over ssh, scp or ansible where op-reason's SSH relay stands in
  front of the 1Password SSH agent (OPREASON_SSH_REASON). Covers how to call op with --reason, how to give ssh a
  reason, what to do when an approval is denied or times out, and how to keep secrets out of the conversation.
---

# op-reason

`op` on this machine is op-reason, a wrapper for the 1Password CLI. Every call:

- needs a reason: `op --reason "<one sentence>" <op arguments…>`;
- starts a fresh process for `op.exe`, so the 1Password approval prompt appears for **every** call and covers that
  call only;
- shows a window next to the prompt with the caller (you), the place (repository, branch, directory), the reason, the
  command and the `op://` references it reads (from the arguments, an `op inject` template and `op run` env files);
  the person approves or denies in 1Password's own prompt;
- is logged (`op-reason-log`).

On Windows the command is `op-reason` (`op-reason.exe`, or `op-reason.ps1` / `op-reason.cmd` where Smart App Control
blocks the exe): write `op-reason --reason …` wherever this guide writes `op --reason …`, and `op-reason --log` for the
log. There, `op` is the 1Password CLI itself, which must never be called directly (below).

## Calling op

```sh
op --reason "read the staging DB password to run the migration" read "op://Private/staging-db/password"
```

- Write the reason as one sentence: what for, and what you read or write. The person reads it before approving;
  a vague reason ("needed", "test") gets denied. The window lists the references itself, so the reason need not
  repeat them; say what they are for.
- `--reason "<text>"` or `--reason=<text>`, anywhere before `--` (after `--`, the arguments belong to op).
- One approval covers one call. Put what a task needs into as few calls as possible (all references of a file in one
  `op inject`, all fields in one `op item get --format json` that you do not print).
- Give the tool call a long timeout: the person may take minutes to answer (in Claude Code, a Bash timeout of
  600000 ms). If your tool stops the call while the prompt is open, the window disappears and only the prompt remains.
- No reason needed (and no prompt): `op --version`, `op --help`, `op <command> --help`.
- Never run `op.exe` (or any other copy of the 1Password CLI) directly: it bypasses the reason, the window and the
  log, and from WSL its approval would be shared by every terminal.
- The full usage: `op` without arguments, or `op-reason --help`.

## When a call fails

| What you see | What happened | What to do |
| --- | --- | --- |
| exit 2, "There is no reason (--reason)" or "The reason (--reason) is empty" | no reason | add one and call again |
| op's `[ERROR] … authorization prompt dismissed` | the person denied the prompt | do **not** call again; ask the user with options (retry now, skip, they run it themselves) |
| op's `[ERROR] … authorization timeout` | nobody answered the prompt | the same: ask, do not call again |
| exit 75, "Another op call is still waiting for its approval" | another call holds the lock while its prompt is open | tell the user; call again after that prompt is answered |
| exit 127 | `op.exe`, `op-reason.exe`, `launcher.ps1`, `powershell.exe` or `wslpath` was not found, or could not be started (the message says which) | tell the user; do not work around it with another op |
| exit 1 with an `op-reason:` line (`could not start op.exe`, `op-reason: launcher: …`, or `op-reason.exe ended without starting the call`) | op could not be started | report the line |
| an `op-reason:` line that says the call runs through `launcher.ps1` instead | op-reason.exe did not start (Smart App Control, for example); the call went on through the fallback | nothing to do for this call; mention it to the user once |

Do not retry denied or timed-out calls: every retry shows another prompt, and people who see many prompts start
approving them out of habit, which defeats the rule "deny any prompt that comes without the window".

## Secrets

- Never print secret values (to the terminal, files, logs or the conversation). `op read` prints the secret: pass it to
  the program that needs it (an environment variable, `op inject` into a file that is not committed) instead of
  showing it.
- Never ask the user to paste a secret. Ask for an `op://` reference (1Password: "Copy Secret Reference").
- Avoid `op run` for long-running programs (servers, watchers): op-reason keeps its lock and window until op exits, and
  every other op call waits (and gives up with exit 75 after 90 seconds). Resolve the values, start the program, and
  let op exit.

## SSH keys (op-reason's SSH relay)

Where op-reason's SSH relay runs (`op-reason.exe --ssh-agent`, in front of the 1Password SSH agent), every call that
signs with a key, such as `ssh`, `git push` or `git fetch` over ssh, `scp` or an ansible run, waits for the person to
allow it in op-reason's window, which shows you as the caller, the command, the destination and the reason.

- Give the reason in `OPREASON_SSH_REASON`, one sentence, for every such command:
  - WSL or another POSIX shell: `OPREASON_SSH_REASON="push the login fix to the review branch" git push`
  - PowerShell: `$env:OPREASON_SSH_REASON = "push the login fix to the review branch"; git push`
- One answer covers one call: everything one command does (all connections of one `git push`, all hosts of one
  `ansible-playbook` run). A later command asks again. Put the work into as few commands as possible.
- Through Claude Code's PowerShell tool on Windows, the window cannot see the script, so it asks for each ssh process
  (each `ssh`, each `git push`) unless the person allows the whole call. Run one command that signs per tool call, with
  its reason, so that each window says what it is for.
- Give the tool call a long timeout (the person may take a minute); the window gives up after 60 seconds, and ssh then
  fails as if the key were refused.
- If ssh fails with `Permission denied (publickey)` or an agent error right after the window, the person denied it or
  did not answer: do **not** run it again; ask the user, as for a denied `op` call.
- Never point ssh at the 1Password agent directly (`\\.\pipe\openssh-ssh-agent`) or change `IdentityAgent` or
  `SSH_AUTH_SOCK` to get around the relay.

## op-reason's settings

`op-reason --settings` (`list`, `get`, `set`, `check`, `relay start|stop|restart`) reads and changes op-reason's own
settings: the relay's mode and times, its scheduled task, the language, where this guide is placed.

- Reading is fine: `op-reason --settings list`, `get`, `check`. `op-reason --settings` without a word opens the settings
  window on the user's desktop; for its words, `op-reason --settings help`.
- Change a setting (`set`, `relay stop`, `relay restart`, `forget-accounts`) only when the user asks for that change.
  Never change one to get past a window, a refusal or a prompt (for example, never switch the relay to `show-only`
  or stop it because a call was denied). Every change is recorded in the log with you as its caller.

## The log

```sh
op-reason-log              # the last hour
op-reason-log --since 1d   # the last day
op-reason-log --json       # one JSON object per call
```

Results: approved, ok (help and service account calls), denied, timed out, failed, refused (no reason, lock wait timed
out, …). On Windows, `op-reason --log` also lists the SSH relay's signatures (SSH allowed, passed, denied, timed out, …).
