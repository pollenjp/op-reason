# Changelog

## 0.6.0 — 2026-11-22

- **After Allow, the SSH relay's window comes to the front until 1Password has answered**, so that 1Password's prompt
  comes at once. 1Password keeps the prompt of an application that it does not see in front behind its icon in the
  taskbar (**SSH request waiting**), and for a relay started from a scheduled task that application is op-reason
  itself: until now its prompt waited there unseen, and the request went unanswered after about 60 seconds. The window
  still opens without taking the focus, **Deny** never takes it, and the window stays behind when the window in front
  belongs to the program the relay was started from (a terminal program). When the request is through, the window that
  had the front gets it back, unless another window took it meanwhile; the relay lets the 1Password app come in front of
  op-reason's window.
- While 1Password asks (after Allow, in show-only mode, or when 1Password is slow to answer), op-reason's window ends
  with where 1Password keeps a prompt it did not show: the 1Password icon at the right of the taskbar → **SSH request
  waiting**.
- The notices "1Password did not answer" and "the relay has no parent process" no longer say that 1Password leaves a
  relay without a parent unanswered and that a scheduled task helps: they say where 1Password keeps its prompt.
- **The relay's windows draw without the GPU** (Direct2D's software renderer, into the window's device context). In a
  check with the real app, the first window of a relay started from a scheduled task stopped answering before it drew
  itself; the windows of `op` calls are unchanged.
- **The relay records a window that does not appear**: when a window's thread has waited five seconds in one step
  (making the window, showing it, drawing it, and so on), a `ssh-relay` record `window-stuck` names the step, and
  `window-unstuck` says when it went on. `op-reason.exe --log` shows them. The `ssh-relay` records get a `detail`.
- The READMEs: why the window comes to the front, and a scheduled task that also repeats every minute, keeping its
  rule "Do not start a new instance" while the relay runs.

## 0.5.0 — 2026-11-20

- **The SSH relay shows a toast for a signature that passes without a window.** In per-call mode, a later signature of
  an allowed call, or a command typed in a terminal that was allowed, shows a small card at the top left of the screen
  for four seconds: `Passed · <caller>`, the ssh command and its destination, why it passed (allowed for this call,
  allowed for this terminal) and the reason. The signatures of one call share one card that counts them (×N) and shows
  the last destination; other calls get a card each, three at most: a fourth pushes the oldest out, and `+N` under the
  third card counts the cards pushed out until four seconds after their last signature. A card is drawn at 90%
  opacity, so that the text behind it shows through faintly. Toasts never take the focus, let clicks through, make no
  sound and stay out of the notification center; with Windows' animation effects off they neither slide nor fade.
- **op-reason's windows open on the screen you work on**: the screen of the foreground window (the primary screen when
  there is none), for the relay's windows and those of `op` calls that `op-reason.exe` runs. Before, they always opened
  on the primary screen.
- **A window's notification goes with its window**, from the screen and from the notification center, so they no
  longer pile up there. A per-call window that timed out unanswered leaves its notification; the one-time notices
  (1Password did not ask, 1Password did not answer, the relay has no parent process) stay. The PowerShell launcher's
  window and notifications are unchanged.
- The READMEs show where the windows, the toasts and the notifications appear, in an animation and in an HTML page
  (`docs/html/relay-screens.html`).

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
