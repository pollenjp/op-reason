# Changelog

## 0.4.0 — 2026-11-18

- **The SSH relay: who and why for every use of an SSH key.** `op-reason.exe --ssh-agent` stands in front of the
  1Password SSH agent: it serves the agent protocol on its own pipe (`\\.\pipe\op-reason-ssh-agent`) and passes every
  request on, unchanged, to 1Password's (`\\.\pipe\openssh-ssh-agent`). Point ssh at it with `SSH_AUTH_SOCK` (or
  `IdentityAgent`, with the leading pair of backslashes doubled; Git for Windows through Windows OpenSSH, with
  `GIT_SSH`), and give the reason in `OPREASON_SSH_REASON`.
  - `per-call` (the default) asks once per call in op-reason's window (Allow this call, Deny, and for `ssh`, `git`,
    `scp`, `sftp`, `rsync` or `ansible` typed in a terminal, Allow this terminal for 60 minutes) and remembers the
    answer for the call, for 60 minutes at most: ansible's connections wait for one answer, and a denied call fails at
    once. A call is its root process (Claude Code's tool call, the command typed in a terminal, or the program that
    runs ssh), the key and the reason; requests forwarded back from a remote host (`ssh -A`) ask one by one. 1Password
    approves the relay once, until it locks. Keep 1Password at "application and terminal session": at "application",
    that approval also lets a client that talks to 1Password directly from the terminal program the relay runs in
    through without a prompt. **Here op-reason's window is where you decide**, unlike for `op`.
  - `show-only` decides nothing: for people whose 1Password asks for every request, it shows who and why next to each
    prompt, and says once when a signature passed too fast for a prompt.
  - The window shows the caller, the root process, the place, the reason, the ssh command, the destination with its
    host key (both host keys for a forwarded connection), what the signature is for and the key's name.
  - Records go to `%LOCALAPPDATA%\op-reason\ssh-log.jsonl` (never the data signed or a signature);
    `op-reason.exe --log` lists them among the calls of `op`.
  - Start it from a scheduled task: 1Password leaves a relay whose parent process has ended unanswered, and the relay
    says so when that happens. `--status` tells whether a relay answers.
  - Outside the relay: git commit signing with `op-ssh-sign`, which talks to the 1Password app directly, and Git
    Bash's own ssh.
- **`op-reason-ssh` on WSL**: the WSL executable under that name runs `ssh.exe` with the facts of the call (the root on
  Linux, the caller, herdr, the place, the distribution and the reason) for the relay, with the arguments and streams
  unchanged. The Nix packages link it.
- The skill tells agents to give ssh a reason.

## 0.3.0 — 2026-11-17

- **The window lists the references a call reads.** Under the command line: the `op://` references of the arguments,
  of the template of `op inject` (its `-i` file, or the template on stdin) and of the `--env-file`s of `op run`; eight
  of them, then "and N more". The log's `start` record keeps them all (`refs`), and `op-reason-log` adds their number
  to the command when they came from a file or stdin. The PowerShell entry lists those of the arguments and files (it
  never reads stdin).
- **A reference whose vault and item are IDs opens the item in the 1Password app.** Press and release on it; the
  window does not take the focus from the approval prompt (also the launcher's window now). The link needs the account
  ID: the end-of-call `op whoami` now runs with `--format json`, and op-reason keeps the account ID and the sign-in
  address (nothing else) in `%LOCALAPPDATA%\op-reason\account.json`. So the first call has no links, and its window
  says why.
- From WSL, `op inject` without `-i` now has its stdin read by op-reason first (when it is not a terminal), which hands
  the same bytes to op. The payload gets an optional `facts.refs` (version 1 still: an older runner shows no list).

## 0.2.0 — 2026-11-16

- **The window chooses the language.** The window of `op-reason.exe` has a switch, `English | 日本語`, at its bottom
  right. A click switches the window at once, without taking the focus from the 1Password prompt, and remembers the
  choice in `%LOCALAPPDATA%\op-reason\settings.json`. Every later call uses it for the window, the notification, the
  messages and `op-reason-log`, on Windows and on WSL (the WSL executable keeps a copy in
  `~/.local/state/op-reason/lang.json`, taken from each call). The launcher's window follows the choice too.
- **English is the default.** `LANG`, `LC_ALL`, `LC_MESSAGES` and the Windows display language no longer choose
  Japanese. `OPREASON_LANG` still works while nothing has been chosen in the window. If you relied on a Japanese
  locale, click 日本語 once, or set `OPREASON_LANG=ja`.
- The status that the runners write gets `chosenLang`. The payload is unchanged (version 1), so a WSL executable and a
  Windows zip of different versions still work together.

## 0.1.0 — 2026-11-15

The first release: the WSL executable (x86_64 and aarch64) and the Windows zip (`op-reason.exe`, the launcher and the
PowerShell entry).
