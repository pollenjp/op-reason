# Changelog

## Unreleased

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
