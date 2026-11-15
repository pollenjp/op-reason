# op-reason: choosing the language in the window — design

- Date: 2026-11-15
- Status: approved (the user chose the proposed design and the recommended answers to its two questions; this
  document records it and the implementer's decisions)
- Builds on: `2026-11-15-windows-exe-design.md` ("the C' spec": the window of `op-reason.exe`),
  `2026-11-15-wsl-through-exe-design.md` (calls from WSL through `op-reason.exe --run`) and §5 of
  `2026-11-15-op-reason-design.md` ("the A' spec": the payload and the status file).

## 1. Goal and non-goals

The user asked that the texts of the notifications and messages can be English or Japanese, English by default, with
the choice made somewhere in the window that appears at the top right.

Goal:

- The window of `op-reason.exe` gets a switch, `English | 日本語`, at its bottom, at the right of the footer line
  ("Deny any 1Password approval prompt that comes without this window").
- A click switches the texts of the window at once and remembers the choice in
  `%LOCALAPPDATA%\op-reason\settings.json`.
- The click neither raises nor activates the window: the 1Password approval prompt keeps the focus.
- From the next call on, the remembered language is the language of the window, the toast, the messages that the WSL
  executable prints and `op-reason-log`. The WSL executable learns the choice from the status file of a call (§5).
- English is the default. `LANG`, `LC_ALL`, `LC_MESSAGES` and the Windows display language no longer choose Japanese.
  The order is: the language chosen in the window, then `OPREASON_LANG`, then English. `OPREASON_LANG` therefore only
  counts while nothing has been chosen, so that a choice made in the window is not undone by an environment variable.

Non-goals:

- A switch in the launcher's WPF window. The launcher (the fallback where Smart App Control blocks the exe, and the
  second half of a call through the PowerShell entry) reads the choice and offers none.
- A command-line option to choose, and other languages.
- The animations of the READMEs (`docs/images/`): they are made again after this change, by their own script.
- A window for Linux, versions and releases.

## 2. Where the language comes from

| Part | Order |
| --- | --- |
| WSL executable (its messages, the usage, `op-reason-log`, the payload's `lang`) | its copy of the choice (`<state>/lang.json`, §5), `OPREASON_LANG`, English |
| `op-reason.exe`, called from Windows (its messages, its window and toast, `op-reason --log`) | `settings.json`, `OPREASON_LANG`, English |
| `op-reason.exe --run` (the window and the toast of a call from WSL) | `settings.json`, the payload's `lang`, English |
| PowerShell entry `op-reason.ps1` / `op-reason.cmd` (its messages, the payload's `lang`) | `settings.json`, `OPREASON_LANG`, English |
| Launcher `launcher.ps1` (its window and toast) | `settings.json`, the payload's `lang`, English |

`OPREASON_LANG` keeps its values: `ja…` is Japanese, anything else that is not empty is English. The payload's `lang`
is what the entry decided by its own row; the runners prefer `settings.json`, which is fresher than the WSL copy.

A call keeps the language it started with for everything it prints, also after a click in its window: the click
changes the window at once and every call after it.

## 3. `settings.json`

`%LOCALAPPDATA%\op-reason\settings.json`, next to the Windows log, UTF-8 without a byte order mark:

```json
{"v":1,"lang":"ja"}
```

- `v`: the version of the file's format, 1. `lang`: `en` or `ja`, the language chosen in the window.
- Only the switch of `op-reason.exe` writes it. It writes a temporary file in the same folder and renames it over the
  old one, so a reader never sees half a file. It keeps the fields it does not know, and `v` when the file has one, so
  that an older exe does not drop what a newer one wrote.
- Readers (`op-reason.exe`, the launcher, the PowerShell entry) use `lang` when it is `en` or `ja`, whatever `v` says (a
  later format that changed the meaning of `lang` would use another name). A missing file, a file that is not a JSON
  object, and a `lang` that is missing or unknown all mean that nothing was chosen. Nothing is printed about it: the
  call goes on in the language of the next row of §2.
- To go back to `OPREASON_LANG` and English, delete the file (§5 says how the WSL copy follows).

## 4. The switch

```text
┌ 1Password CLI approval request ───────────────────────────────── × ┐
│ 1Password CLI approval request                                      │
│ Time: …                                                             │
│ …                                                                   │
│ Deny any 1Password approval prompt that comes without               │
│ this window                                       English | 日本語  │
└─────────────────────────────────────────────────────────────────────┘
```

- Text: `English | 日本語`, Segoe UI at 11 DIPs (the lines are 12). The language shown is bold and black and does
  nothing when clicked; the other one is blue and underlined, like a link, and shows the hand cursor. The bar is gray.
- Place: at the right edge of the text area, on the baseline of the footer's last line. The footer is laid out
  narrower by the width of the switch and a 12 DIP gap, so the English footer takes two lines; the window is as tall
  as the paragraphs need, as before.
- The clickable part is the other language's word, its line box widened by 4 DIPs on each side and 3 DIPs above and
  below. A click counts when the button goes down and comes up on it.
- What a click does, all within the window's thread:
  1. the title, every line, the orange "other op.exe" line, the footer and the switch are laid out again in the other
     language (the orange line keeps its state), the caption changes, and the window takes its new height, keeping
     its top right corner;
  2. the choice is written to `settings.json`. When it cannot be written (no `LOCALAPPDATA`, a file in use), the
     window still switches, and nothing is said.
- The window never takes the focus: it keeps `WS_EX_NOACTIVATE` and answers `WM_MOUSEACTIVATE` with `MA_NOACTIVATE`,
  so the click reaches it without activating it, and every resize uses `SWP_NOACTIVATE`. The 1Password prompt stays
  the active window and keeps the keyboard.
- Positions are kept in DIPs and the mouse's pixels are scaled by the window's DPI, so the switch works on every
  monitor (Per-Monitor V2, as the rest of the window).
- The toast is shown once, at the start, in the language of the call; a click does not change it. The status's `ui`
  keeps the texts the call started with.

## 5. The WSL executable's copy

The WSL executable cannot read `%LOCALAPPDATA%` without starting a Windows program, so the runners tell it the choice:

- `op-reason.exe --run` and the launcher add `chosenLang` to the status they write at the end: `en` or `ja` as read from
  `settings.json` at that moment (after any click in this call), or null when nothing is chosen. A runner that cannot
  tell (no `LOCALAPPDATA`, a read error other than a missing file) leaves the field out.
- After each call, the WSL executable updates `<state>/lang.json` (`${XDG_STATE_HOME:-~/.local/state}/op-reason/`) from
  the status: `en` or `ja` → it writes `{"v":1,"lang":"…"}` (a temporary file renamed, only when it changes); null → it
  removes the file; no field (an older runner), another value or no status → it leaves the file as it is.
- `lang.json` is a copy, not a setting: it is the first row of the WSL executable in §2, and the next status replaces
  it. After `settings.json` is deleted, the next call still starts in the copied language and removes the copy; the
  call after it follows `OPREASON_LANG`. Deleting both files resets at once.

## 6. Contract and mixed versions

The WSL executable and the Windows zip are installed separately (the dotfiles pin a version of each), so for a while
either side may be older. The payload stays as it is (`v` 1, no new field: an older exe refuses another version), and
the status gets one optional field, `chosenLang`; every reader ignores fields it does not know.

| WSL executable | Windows zip | What happens |
| --- | --- | --- |
| new | new | as above |
| new | old | the old exe and launcher show the payload's `lang` (the WSL copy, else `OPREASON_LANG`, else English) and write no `chosenLang`: the copy stays as it is |
| old | new | the new exe and launcher follow `settings.json`, else the payload's `lang`; the old WSL executable keeps its old rules (`OPREASON_LANG`, then `LC_ALL`/`LC_MESSAGES`/`LANG`) for its own messages until it is updated |
| old | old | as before |

An older exe ignores `settings.json`, so going back to an older zip leaves the file for a later update.

## 7. Messages, documents

- No new message: a click prints nothing, and a broken `settings.json` is quiet (§3).
- The READMEs: the `OPREASON_LANG` row and a short section on the switch. `CHANGELOG.md` (new) says under "Unreleased"
  that English is the default and the language is chosen in the window.
- `docs/architecture/README.md`: `chosenLang` in the status, `settings.json` on Windows and `lang.json` in the state
  directory. The A' spec §5.4: the `chosenLang` row.

## 8. Tests

- Core (`cargo test`): the order of §2 (`LANG` no longer counts); `settings.json` and the copy: read (missing, broken,
  unknown `lang`, any `v`), write (keeps unknown fields and `v`, replaces a broken file); the status's `chosenLang`:
  missing, null, a value, and a value of the wrong type that must not make the whole status unreadable.
- `tests/wrapper.test.sh` (fake runner writing `chosenLang`): a status with `en` writes `lang.json` and the next call
  speaks English despite `OPREASON_LANG=ja` (messages, payload `lang`, `op-reason-log`); null removes it; no field
  leaves it; `LANG=ja_JP.UTF-8` alone gives English.
- `tests/launcher.test.sh` (pwsh on Linux, `LOCALAPPDATA` pointed at the work folder): `settings.json` with `en` beats
  the payload's `ja` in `ui` and is reported as `chosenLang`; no file or a broken file → the payload's language and
  null.
- `tests/e2e.test.sh`: the WSL executable with the real launcher: the launcher's `chosenLang` reaches `lang.json`, and
  the next call follows it.
- `crates/op-reason-win/tests/exe.rs` on Windows: the entry face and `--run` follow `settings.json` over
  `OPREASON_LANG` and the payload, `--run` reports `chosenLang`; and, on CI only (it moves the mouse), a real click on
  `日本語` in an English window: the caption becomes Japanese, `settings.json` says `ja`, the foreground window is the
  same as before the click and is not ours.
- `tests/windows/entry.test.ps1` and `launcher.test.ps1`: the PowerShell entry and the launcher follow
  `settings.json`.
- The screenshot job: every scene shows the switch, and one more scene is taken after a click on `日本語`.

## 9. Manual checks

`docs/manual-verification.md` gets the checks L1–L7: the default is English even with `LANG=ja_JP.UTF-8`; a click
switches the window and leaves the focus on the 1Password prompt; the next call, its messages and `op-reason-log` follow
the choice, also over `OPREASON_LANG`; a call from Windows follows the same file; the launcher reads it; deleting the
files brings `OPREASON_LANG` back.

## 10. Implementer's decisions

| Decision | Why |
| --- | --- |
| `settings.json` is `{"v":1,"lang":…}`; readers ignore `v`, writers keep unknown fields and `v` | an older and a newer exe can take turns on one file without losing each other's fields |
| A missing, broken or unknown `settings.json` is quiet and means "nothing chosen" | a bad file must not stop a call; the next click writes a good one |
| Written through a temporary file and a rename, retried for a moment when the file is in use | readers never see half a file; PowerShell opens it without sharing the right to replace it |
| The payload does not change; the status gets `chosenLang` (null = nothing chosen, missing = cannot tell) | an older exe refuses another payload version; old readers ignore an extra status field; null and missing let the copy be removed only when the runner knows |
| A malformed `chosenLang` is ignored, not a reason to reject the status | the status decides the result of the call |
| The WSL copy is `<state>/lang.json`, named for what it holds | it is a copy, not a settings file to edit |
| The runners prefer `settings.json` over the payload's `lang` | the file is fresher than the WSL copy (a choice made in a call from Windows) |
| A call keeps its starting language for its own messages after a click | one language per call; the window and every later call change at once, as the design asks |
| The switch: 11 DIPs, the shown language bold and black, the other blue, underlined and with the hand cursor | small, as asked, and the clickable word looks clickable |
| The switch sits on the baseline of the footer's last line, the footer narrower by the switch | "at the right of the footer line", at the bottom right; the window keeps its height rules |
| The clickable word's box widened by 4 DIPs sideways and 3 DIPs up and down; a click is a press and a release on it | an 11 DIP word is a small target |
| The toast and the status's `ui` keep the call's starting language | a shown toast cannot be changed; `ui` records what the call built |
| The real click is tested on CI only | it moves the mouse of whoever runs the tests |
| `CHANGELOG.md` starts with this change | the default language changes for existing users, and the release notes have no other place for it |
| No version change in this pull request | releases are decided separately |
