# Changelog

## Unreleased

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
