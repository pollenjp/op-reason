---
name: op-reason
description: >-
  Use when running the 1Password CLI (`op`) on a machine where `op` is op-reason, a wrapper that requires a reason
  for every call (WSL or Windows), or when an `op` call was refused for a missing reason or ended with an op-reason
  exit code (2, 75, 127). Covers how to call op with --reason, what to do when an approval is denied or times out,
  and how to keep secrets out of the conversation.
---

# op-reason

`op` on this machine is op-reason, a wrapper for the 1Password CLI. Every call:

- needs a reason: `op --reason "<one sentence>" <op arguments…>`;
- starts a fresh process for `op.exe`, so the 1Password approval prompt appears for **every** call and covers that
  call only;
- shows a window next to the prompt with the caller (you), the place (repository, branch, directory), the reason and
  the command; the person approves or denies in 1Password's own prompt;
- is logged (`op-reason-log`).

## Calling op

```sh
op --reason "read the staging DB password to run the migration" read "op://Private/staging-db/password"
```

- Write the reason as one sentence: what for, and what you read or write. The person reads it before approving;
  a vague reason ("needed", "test") gets denied.
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
| exit 127 | `op.exe`, `powershell.exe` or `wslpath` was not found | tell the user; do not work around it with another op |
| exit 1 with an `op-reason: launcher:` line | the launcher could not start op | report the line |

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

## The log

```sh
op-reason-log              # the last hour
op-reason-log --since 1d   # the last day
op-reason-log --json       # one JSON object per call
```

Results: approved, ok (help and service account calls), denied, timed out, failed, refused (no reason, lock wait timed
out, …).
