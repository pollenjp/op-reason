# Changelog

## Unreleased

- **The SSH relay's window shows the call's processes as a tree**, parent above child, from the caller (Claude Code,
  the person's shell, or the program) down to the ssh process, each with its PID. The command's box sits on the row of
  the process whose words it shows: the root command under the root's row (marked "Root"), with the rest of the root's
  line and the ssh command in small type; for Claude Code's PowerShell tool, the ssh command under the ssh process's
  row. Frames on the tree show what each button covers. A WSL call shows what `op-reason-ssh` sends, then "… in WSL,
  not seen"; with more than four processes between, all but the first and the last two fold into one row.
- **A call of Claude Code's PowerShell tool asks for each ssh.** Its script reaches no command line, so the second ssh
  of such a call passed without a window, seen only in a toast. Now **Allow this ssh** (the first button) covers that ssh
  process until it ends, and another ssh of the call asks in a window of its own; **Allow this call** covers the call as
  before, and **Deny** refuses it.
- `ssh-sign` records: the answer `allow-ssh`, and `by`, the answer that covered a request that passed or was refused
  without a window; `op-reason --log` says "(this ssh)". The guide for agents (`op-reason --skill`) says that the
  PowerShell tool asks for each ssh.

## 0.8.1 — 2026-11-27

- **The SSH relay's window for a call of Claude Code's PowerShell tool** on Windows shows the ssh command in its box,
  with the root process and "Claude Code's PowerShell tool" under it, and says "Allowing covers this call until it
  ends". The tool runs its script through a launcher that carries no command (`cmd.exe /d /s /c` running
  `chcp.com 65001`, then PowerShell with a fixed `-Command` that runs the script from an environment variable): the
  window showed the launcher's line as the root command and named "pwsh.exe -NoProfile -NonInteractive …" in the line
  about how long an answer lasts. Only a call of Claude Code is shown so, when the launcher's own arguments (not the
  record's masked line) match word for word.

## 0.8.0 — 2026-11-26

- **A modern look for `op-reason.exe`'s windows**, in Windows' dark or light app mode (it follows a change while a
  window is open): the reason first, in large letters (or "no reason given" in its place); the caller, herdr, the WSL
  distribution, the repository with its branch and the directory as chips; the command in a monospace box with the
  program and the file names in color; a band with a small picture of the screen that marks where 1Password's prompt
  usually opens and the tray where it waits otherwise; the language switch as a pill. The caption keeps its close
  button but no title. A window taller than its screen draws its reason smaller, then leaves the band out, then cuts the
  command's box, so that its buttons stay on the screen. The launcher's window keeps its look.
- **The SSH relay's window** shows the root command first (`git push origin fix/login`: the wrappers of a shell and of
  Claude Code's Bash tool are taken off when they match exactly, and the root process with the rest of its line stays
  under the box), the ssh command in small type, and a bar of the seconds left to answer. Its line about how long an
  answer lasts names the root command: "Allowing covers this git push until it ends (at most 60 minutes)" (it said "an
  answer covers this call for 60 minutes", which read like a permission for 60 minutes). **Allow this terminal for 60
  minutes** is unchanged.
- The relay's toasts take the windows' colors, and show the command and its destination in a monospace font.
- **After a click on Allow, the relay's window watches for 1Password's prompt**: when a window of 1Password comes (to
  the front, or new), it says "Approve in 1Password's window" with a sketch of the question; when none has come within
  two seconds (1Password kept its prompt behind its icon once, for a request while it was locked), the window lights
  up, counts down the seconds until 1Password closes the request, and a small click-through window next to the
  notification area points at the 1Password icon in the tray (SSH request waiting). The `ssh-sign` record of such a
  request says what the relay saw (`prompt`: `front`, `seen`, `hint`).

## 0.7.0 — 2026-11-24

- **`op-reason --settings`**: one list of op-reason's settings with the same three operations, `get` (the value now),
  `set` (make it so; running it again changes nothing) and `check` (compare a wanted value with the value now: exit 0
  the same, 1 different, 2 failed), with `--json`, and `list`, `relay start|stop|restart`, `forget-accounts` and
  `open-records`. The settings: `lang`, `relay.mode`, `relay.time-limit`, `relay.answer-wait`, `relay.tray-icon` (in
  `settings.json`), `relay.autostart`, `relay.running`, `start-menu`, `skill.windows`, `skill.wsl.<distribution>` and
  `skill.folder.<folder>`. On WSL, `op-reason --settings` runs `op-reason.exe --settings`; the PowerShell entry says that
  the settings come from `op-reason.exe`.
- **op-reason's own scheduled task for the SSH relay** (`relay.autostart`): `\op-reason-ssh-relay-<SID>` in the root
  folder, with the shape the README recommends (at logon and again every minute, "Do not start a new instance", hidden,
  priority 5), which starts `op-reason.exe --ssh-agent --task <its name>`: the relay writes the name into `relay.json`, and
  only a relay that does so is op-reason's to stop. `relay stop` turns the task off and ends the relay until
  `relay start`. Stops and restarts take op-reason's lock
  first, and a relay that another task or a terminal started is shown, never stopped; while one runs, op-reason's task is
  made turned off.
- **The relay reads its mode and times from `settings.json`**; its options still win. `--time-limit` takes at most a
  day and `--answer-wait` at most an hour (a longer value made the relay fail at its first window). It writes `relay.json` (its
  process ID and the values it runs with), and on its stop event it fails new requests, lets those with 1Password finish
  (25 seconds at most) and exits; `op-reason --log` shows the stop.
- **The skill placed where agents read it**: `skill.windows` (`%USERPROFILE%\.claude\skills`), `skill.wsl.<distribution>`
  (through `wsl.exe`) and `skill.folder.<folder>`. The copy carries op-reason's version after its front matter, is updated
  when older and removed only where it carries the mark; nothing is written through a junction or a symbolic link.
- Every settings change is recorded in `log.jsonl` with its caller, and `op-reason --log` shows it. The skill tells agents
  to change op-reason's settings only when the user asks.
- **The settings window**: `op-reason --settings` without more words, and a double click on `op-reason.exe`, open a
  window of Windows' standard controls (one per logon) that does what the command line does, recorded with
  `from: window`: the language, the SSH relay (Start, Stop and Restart as op-reason may act, the task, the icon, the
  mode and the times with Apply), the skill places (with a folder picker), the Start menu entry and the records.
- **`start-menu`**: `op-reason.lnk` in the Start menu, so that Windows' search finds the settings window.
- **The relay's icon in the notification area** (`relay.tray-icon`, on by default, applied at once): a click opens the
  settings window; its menu opens the records folder, restarts or stops op-reason's relay and hides the icon (the
  changes are recorded with `from: tray`). It comes back when Explorer starts again.
- **No console window when Task Scheduler starts the SSH relay** (at logon, within a minute, after **Apply** in the
  settings window and **Restart the relay** in the icon's menu, and the relay of another task), on Windows 11 24H2 and
  later and Windows Server 2025: `op-reason.exe`'s manifest asks Windows for no console unless it inherits one
  (`consoleAllocationPolicy` `detached`). Until now Windows made the relay a console of its own and showed it, or
  handed it to Windows Terminal, whose window came to the front, until the relay let go of it. A double click on
  `op-reason.exe`, a start of it pinned to the taskbar and the Start menu's **op-reason** open the settings window
  without a console window too. Older
  Windows keep the console for a moment. Terminals and calls from WSL see no change. A call that a program without a
  console starts with pipes shows no console window for `op.exe` either: when `op-reason.exe` has no console, `op.exe`
  gets one that is never shown. The relay's `start` record names the console it had (`detail`: `console: none`,
  `console: own` or `console: shared`).

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
