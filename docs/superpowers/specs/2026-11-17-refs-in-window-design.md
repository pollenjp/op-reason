# op-reason: the references a call reads, in the window — design

- Date: 2026-11-17
- Status: approved (the user chose the proposed design: plan A for the account, and a line that says why the first
  call has no links; this document records it, the result of the check with the real 1Password app, and the
  implementer's decisions)
- Builds on: `2026-11-15-op-reason-design.md` §5 (the payload, the status and the log), `2026-11-15-windows-exe-design.md`
  (the window of `op-reason.exe`), `2026-11-15-wsl-through-exe-design.md` (calls from WSL through `op-reason.exe --run`)
  and `2026-11-15-language-toggle-design.md` (the clickable switch that never activates the window).

## 1. Goal and non-goals

The user's request: the window says that a call runs `op inject`, but not which items and fields it reads. List the
`op://` references in the window, and let a click open the item in the 1Password app.

Goal:

- Under the command line, the window gets a block "References to read (N, from …)" / 「読む参照 (N 件・… から)」 with
  one reference per line, at most 8; past 8 a line "and N more" / 「ほか N 件」. The log keeps them all.
- A reference whose vault and item are both IDs is a link: blue, underlined, the hand cursor. Pressing and releasing
  the mouse on it opens the item in the 1Password desktop app. A reference with names, with a variable (`$VAR`), or
  for an account op-reason does not know is plain text.
- A click neither raises nor activates the window: the 1Password approval prompt keeps the focus, as with the language
  switch.
- `op-reason.exe` (both faces) and the launcher's WPF window (the fallback where Smart App Control blocks the exe, and
  the second half of a call through the PowerShell entry) show the same block and the same links.
- When the block has an ID reference but op-reason does not know the account yet (the first call), one line says why
  there are no links and when they appear.
- References are not secrets: the window and the log show them. Values are never shown.

Non-goals:

- Items named in other ways (`op item get <item> --vault <vault>`, `op document get`): only `op://` references.
- The references that `op run` finds in environment variables (a call from WSL does not pass its environment to
  `op.exe` anyway), and turning names into IDs.
- The toast: its texts stay as they are.
- The reason that the dotfiles' `pjp-op-env` writes ("読む参照 …"): it can drop the list later, in a change of its own.
- The README animations (`docs/images/`): the screenshots of the pull request show the new block.
- A version, a tag or a release.

## 2. Where the references come from

Only desktop calls collect references (help and service calls show no window). The sources, in this order:

| Source | What is read | Entries that read it |
| --- | --- | --- |
| Arguments | every argument before `--` that starts with `op://` (the whole argument) | all three |
| `op inject` template | the file of `-i`/`--in-file`; without one, stdin | file: all three; stdin: the WSL executable and `op-reason.exe` only |
| `op run` env files | every `--env-file` | all three |

- **Finding the subcommand.** The first argument before `--` that is not a flag, skipping the values of op's global
  flags that take one (`--account`, `--config`, `--encoding`, `--format`, `--session`; the `=` forms carry their value).
- **`op inject` flags.** `-i X`, `-iX`, `-i=X`, `--in-file X`, `--in-file=X`; `-o`/`--out-file` and `--file-mode` take a
  value; shorthand flags may be combined as pflag allows (`-fi X`). Without `-i`, op reads the template from stdin.
- **`op run` flags.** `--env-file X` and `--env-file=X`, repeatable; `--environment` takes a value. Arguments after `--`
  are the command's, not op's: they are not read.
- **`--account`.** The value of the last `--account` before `--` is kept for choosing the account of the links (§5.3).
- **Stdin.** The WSL executable and `op-reason.exe` read stdin only for `op inject` without `-i`, only when stdin is not
  a terminal (a console on Windows), to the end, before the lock, and hand the same bytes to op (the WSL executable
  already does this for a pipe from another op-reason, design §6.4). The PowerShell entry never reads stdin: there, a
  template on stdin is not listed.
- **Files.** Named as op gets them, read relative to the entry's current directory: regular files only, at most the
  first 8 MiB, as UTF-8 (invalid bytes replaced). A file that cannot be read gives no references and no message (op
  will say why it cannot read it). A Windows path in a call from WSL is not read.

### 2.1 Template syntax

As in 1Password's documentation ("Template syntax", "Secret reference syntax"):

- **Unenclosed**: `op://` not preceded by a letter, a digit, `-`, `+`, `\` or `.`; it runs while the characters are
  letters, digits, `-`, `?`, `_`, `.`, `/` or `=`, or a variable (`$NAME`, `${…}`). The documentation's list omits `/`
  and `=`, but its own examples (`op://vault/item/field`, `?attribute=otp`) contain them.
- **Enclosed**: `{{`, optional spaces, a reference, optional spaces, `}}`, on one line. Everything between the spaces
  is the reference: spaces inside it are kept (the secret reference syntax allows them in names).
- **Escapes**: `{{ "…" }}` (with `\"` inside) is literal text: nothing in it is read.
- **Variables**: `$NAME` (a letter or `_`, then letters, digits and `_`), `${NAME}`, `${NAME:-default}`. They stay as
  written; such a reference is never a link.

### 2.2 Env files

One `KEY=VALUE` per line; blank lines and lines starting with `#` are skipped; `export ` in front is ignored; a value
in double quotes (with `\"` inside) or single quotes is taken without them; an unquoted value ends before ` #`. A value
that starts with `op://` is one reference (spaces kept); any other value is read as a template (§2.1).

### 2.3 How a reference is shown

Control characters and the characters that change the direction of text (U+200E, U+200F, U+202A–U+202E,
U+2066–U+2069) become spaces, so that a reference cannot rewrite the window or look like another one; spaces at both
ends are trimmed. The list keeps each distinct reference once, in the order first seen.

## 3. Contract

### 3.1 The payload: `facts.refs`

`facts` gets one optional field, sent only when the call reads at least one reference:

```json
"refs": {
  "total": 11,
  "from": [{ "kind": "args" }, { "kind": "file", "name": "app.env.tpl" }, { "kind": "stdin" }, { "kind": "env-file", "name": ".env" }],
  "list": ["op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/password", "op://Private/proxmox/token"],
  "account": "example"
}
```

- `total`: the number of distinct references. `from`: the sources that gave at least one, in the order of §2; `name`
  is the file's last path component (at most 40 characters). `list`: the first 8, each at most 200 characters.
  `account`: the call's last `--account` value (at most 100 characters), when given.
- The contract version stays 1. Every reader ignores fields it does not know (serde's default, the launcher's
  `ConvertFrom-Json`), so an older runner shows no block and a newer runner without `refs` shows none either.

### 3.2 The status: `ui.refs`

The status's `ui` (the texts the window was built from) gets `refs` when the block is shown:
`{"head": "…", "items": [{"text": "…", "linked": true}], "more": "…" | null, "hint": "…" | null}`. `linked` says that
the line is a link; the URL itself stays out of the status.

### 3.3 The log: `start.refs`

The `start` record gets `refs` when the call reads at least one: `total`, `from`, and `list` with every distinct
reference (at most 1000, each at most 300 characters). `op-reason-log --json` shows it as it is. The table adds the
count to the command, `(5 refs)` / `(参照 5 件)`, when a source other than the arguments gave references (those in the
arguments are in the command already); the count is never cut off with the command.

## 4. The window

```text
┌ 1Password CLI approval request ────────────────────────────────────── × ┐
│ Command: op inject -i app.env.tpl                                         │
│ References to read (11, from app.env.tpl):                                │
│     op://abcdefghijklmnopqrstuvwxyz/0123456789abcdefghijklmnop/password   │  ← link
│     op://Private/proxmox/token                                            │
│     …                                                                     │
│     and 3 more                                                            │
│     Links to the items appear once an approved call has finished (…)      │  ← only without an account
│ Note: …                                                                   │
└───────────────────────────────────────────────────────────────────────────┘
```

- **Place.** Right after the command line, before the notes.
- **Head.** Bold, like the labels of the other lines, with `:` after it: 「読む参照 (11 件・app.env.tpl から)」, "References to
  read (11, from app.env.tpl)". The sources: 「引数」/"arguments", 「標準入力」/"stdin", the file names; joined with
  「、」/", ".
- **Lines.** Indented by 12 DIPs, Segoe UI 12 like the other lines, wrapped when long. A link is `#0066CC` and
  underlined (the switch's blue); other references are black. 「ほか N 件」/"and N more" and the hint are dark gray
  (`#555555`).
- **The hint.** Shown when at least one of the references shown is an ID reference (§5.1) and no account is chosen
  (§5.3): 「項目へのリンクは、承認が通った呼び出しを 1 回終えると付きます (そのとき、op が既定で使うアカウントの ID を覚えます)」,
  "Links to the items appear once an approved call has finished (op-reason then learns the ID of the account op uses by
  default)". It is not shown when no reference shown could be a link.
- **The click** (`op-reason.exe`): the clickable part is the link's text (DirectWrite's hit test); the hand cursor shows
  over it. A click counts when the left button goes down and comes up on the same link. The window keeps
  `WS_EX_NOACTIVATE` and answers `WM_MOUSEACTIVATE` with `MA_NOACTIVATE`, as for the switch.
- **The click** (the launcher): a WPF `Hyperlink` (not focusable, so that WPF does not call `SetFocus`, which would
  activate the window), handled on `Click`. The window gets a message hook that answers `WM_MOUSEACTIVATE` with
  `MA_NOACTIVATE`: the message filter of WPF's own `Popup` (compiled code that does exactly that, found by reflection),
  because a PowerShell script block cannot be that hook (PowerShell does not write back the hook's `ref bool handled`,
  so the window still activated on CI) and a WPF window cannot get `WS_EX_NOACTIVATE` without compiled code (an
  `HwndSource` created with that style did not show on the Windows runner). When the filter is missing, a click
  activates the window, as it always could. The launcher's window has had no clickable part until now.
- The texts are in both tables (`ui.rs` and the launcher's `Get-Texts`, kept equal by `table_matches_the_launcher`):
  `refs`, `refsArgs`, `refsStdin`, `refsSep`, `refsMore`, `refsHint`.

## 5. Links

### 5.1 The URL

`onepassword://view-item?a=<account ID>&v=<vault ID>&i=<item ID>`. A reference is an ID reference when it starts with
`op://`, then exactly 26 lowercase letters or digits (the vault), `/`, exactly 26 lowercase letters or digits (the
item), and then `/`, `?` or the end. The URL is built from these three IDs only; nothing else of the reference or of
any template goes into it.

The check with the real app (the user, on Windows, one account; a browser page that built the candidates from a
private link of one item and opened each with the Windows handler, so that the link never left the browser):

| # | URL | Result |
| --- | --- | --- |
| 1 | `onepassword://view-item?a=A&v=V&i=I` | opened the item |
| 2 | `onepassword://view-item?v=V&i=I` | did not open it |
| 3 | `onepassword://open/i?a=A&v=V&i=I&h=H` | opened the item |
| 4 | `onepassword://open/i?a=A&v=V&i=I` | did not open it |
| 5 | `onepassword://open/i?v=V&i=I&h=H` | did not open it |
| 6 | `onepassword://open/i?v=V&i=I` | did not open it |

The shapes it reported: `a` 26 uppercase letters and digits, `v` and `i` 26 lowercase letters and digits, `h` a host
under `1password.com`. Both forms need the account ID; `view-item` needs nothing else (`open/i` needs the sign-in
address too), and it is the form 1Password's own VS Code extension opens. So: `view-item`, with the account ID.

The Windows shell hands the handler `onepassword://view-item/?a=…` (it adds the `/` of the empty path; seen on CI, from
both `ShellExecuteW` and `Process.Start`). Browsers open external links through the same shell (`ShellExecuteEx`), so
that is presumably also what the app received in the check above. With a build of this change (§12), a click opened
the item from both windows through the same shell.

### 5.2 The account copy (plan A)

op-reason learns the account ID without a new prompt and without a new `op.exe` call:

- Every runner already ends a desktop or help call with `op whoami` (then `signout --all` when signed in). It now runs
  `op whoami --format json` and reads two fields of the output: `account_uuid` (kept when it is 26 letters and digits)
  and the host of `url` (lowercase, letters, digits, `.` and `-`; empty when it is not a host). Nothing else of the
  output is kept or written (it also holds the email address and the user ID). The exit code means what it meant.
- They are written to `%LOCALAPPDATA%\op-reason\account.json` (next to `settings.json`), UTF-8 without a byte order
  mark: `{"v":1,"accounts":[{"id":"ABCDEFGHIJKLMNOPQRSTUVWXYZ","host":"example.1password.com"}]}`. The account just
  seen goes first; at most 5 are kept; the file is written to a temporary file in the same folder and renamed over the
  old one. Unknown fields are not kept: the file is a copy that the next call rewrites.
- Writers: `op-reason.exe` (both faces) and the launcher, after a whoami that succeeded. Not written: service calls (no
  whoami), calls with `lock: skipped` (they do not close the sessions), a whoami that failed or whose output has no
  usable account ID.
- Readers: the same, when they build the window. A missing or broken file means "no account": the hint, no links.
  Deleting the file removes the links until the next approved call.

### 5.3 Choosing the account

- With `facts.refs.account` (the call's `--account`): the first remembered account whose ID equals it (ignoring
  case), whose host equals it (without `https://` and a trailing `/`), or whose host's first label equals it (the
  shorthand op gives an account by default). None: no links, and the hint.
- Without it: the first account, the one the last whoami reported.
- `op whoami` reports the account op uses by default (or `OP_ACCOUNT`'s): a call with `--account` for another account
  does not teach op-reason that account, so its references stay plain (the hint says that the default account is the
  one learned).

### 5.4 Opening

- Only `onepassword://view-item?` URLs built as in §5.1 are opened; the account ID is checked again when the URL is
  built (26 letters and digits).
- `op-reason.exe`: on the release of the click, `AllowSetForegroundWindow(ASFW_ANY)` (the process received the click,
  so it may pass on the right to come to the front, and 1Password's own window can then come forward), then
  `ShellExecuteW("open", url)` on a thread of its own with COM initialized as single-threaded (as Microsoft documents
  for `ShellExecute`), so that the window keeps running whatever the shell does. When the window closes, it waits up
  to two seconds for such a thread.
- The launcher: `System.Diagnostics.Process.Start(url)` (`ShellExecute` underneath) from the `Click` handler. It calls
  no Win32 function itself (the launcher compiles no code); Windows lets a process started from a click come to the
  front.
- A click while the 1Password approval prompt is up opens the item in the app: that is what the click is for. What
  the app then does with its prompt cannot be tested in CI; `docs/manual-verification.md` gets the check. Seen with
  the real app (§12): the app comes to the front with the item, and its prompt stays in front of it and can still be
  answered; while 1Password is locked, a click changes nothing (the app stays locked, in the back).

## 6. Mixed versions

The WSL executable and the Windows zip are installed separately, so either may be older for a while.

| WSL executable | Windows zip | What happens |
| --- | --- | --- |
| new | new | as above |
| new | old | the old exe and launcher ignore `facts.refs`: no block; the old whoami writes no `account.json` |
| old | new | no `facts.refs` in the payload: no block for calls from WSL; calls from Windows show it; the copy is written by every new runner |
| old | old | as before |

## 7. Safety

- URLs are built only from the three IDs, each checked against its character set; nothing of a template or a reference
  goes into a URL, and only `onepassword://view-item?` URLs are opened.
- `account.json` holds only account IDs and sign-in hosts. The whoami output (with the email address and the user ID)
  stays in memory and goes nowhere else.
- References are shown and logged (they are not secrets); values never are. The templates and env files are read, not
  kept: only the references found in them travel on.
- Characters that could rewrite or disguise a line become spaces (§2.3).

## 8. Messages, documents

- No new message on stdout or stderr.
- READMEs: the block, the links and the copy (what it holds, how to delete it, the default-account rule).
- `docs/architecture/README.md`: `facts.refs`, `ui.refs`, `start.refs`, `account.json`, whoami with `--format json`,
  stdin read for `op inject`.
- `docs/manual-verification.md` (and `.ja`): the checks of §10.
- `skill/SKILL.md`: the window lists the references, so a reason need not list them.
- `CHANGELOG.md`: under "Unreleased".

## 9. Tests

- `tests/cases/refs.json`: calls (arguments, files, stdin) and the references, sources and account they give, and the
  ID references. Read by the core tests (`crates/op-reason-core/tests/refs.rs`) and by the PowerShell entry's test
  (`tests/windows/entry-plan.test.ps1`, plan-only mode, which skips the cases that need stdin).
- Core unit tests: the scanner (§2.1), the env file parser (§2.2), the shown form (§2.3), the payload part (8 of N, the
  clips), the log part, `link_ids`, the log viewer's count.
- `crates/op-reason-win` unit tests (Linux): the texts of the block in both languages, the hint rule, the URL, the
  account copy (whoami output read, merged, written, chosen).
- `tests/wrapper.test.sh` (fake runner): `facts.refs` for an argument, a template file, a template on stdin (and the
  same bytes reach the runner), env files; nothing from stdin when `-i` is given; `start.refs` in the log; the count in
  `op-reason-log`.
- `tests/launcher.test.sh` (pwsh on Linux): `ui.refs` with and without `account.json`, the hint, `--account`; the
  whoami JSON writes `account.json` with only the ID and the host.
- `tests/e2e.test.sh`: the WSL executable and the real launcher: references from a template on stdin reach `ui.refs`;
  a second call has links after the first call's whoami.
- `crates/op-reason-win/tests/exe.rs` (Windows): both faces show the block (`ui.refs`), the entry face reads a template
  on stdin and op.exe gets the same bytes, whoami writes `account.json`; on CI only (it moves the mouse and registers a
  handler for `onepassword:` in the runner's HKCU, unless one exists): a real click on a link opens exactly the
  expected URL (a fake handler records it; the `/` the shell adds is accepted) and does not make the window the
  foreground window. The runner's desktop shows no pointer, so the test finds the link by clicking down the left of
  the text until the handler has written.
- `tests/windows/launcher.test.ps1` and `entry.test.ps1`: the same for the launcher (with the CI-only click) and the
  PowerShell entry (references from arguments and files, never stdin).
- The screenshot jobs: windows with linked and plain references, "and N more", and the hint.

## 10. Manual checks

`docs/manual-verification.md` gets F1–F7 (the letter R is taken): the block of a template file, with the hint on the
first call; the account copy after an approved call; a link that opens the item while the prompt is up, with
1Password unlocked and locked, the window staying inactive; a template on stdin through `op-reason.exe`; the launcher
(the fallback) doing the same; `op-reason-log` with the count and `--json` with the list. Every prompt but one is
denied: the window shows before the prompt, so nothing is read.

## 11. Implementer's decisions

| Decision | Why |
| --- | --- |
| `view-item` with the account ID | it opened the item in the real check; it needs no sign-in address; 1Password's VS Code extension uses it |
| The copy keeps the ID and the host; the URL uses only the ID | the host lets `--account <address>` and `--account <shorthand>` find the account; nothing else is kept |
| whoami runs with `--format json`, no other change | the account ID is in its JSON output; no new call, no new prompt, the same exit codes |
| No `--account` added to whoami | `whoami` must keep not prompting; the default account is the common case, and other accounts' references stay plain |
| Up to 5 accounts, most recent first | the default account can change (`OP_ACCOUNT`); a short list is enough |
| One hint text, which says the default account is the one learned | true for the first call and for `--account` of another account alike |
| Only desktop calls collect references | the block is for the window; help and service calls have none |
| Stdin read only for `op inject` without `-i`, and not from a terminal | that is the only case where op reads a template from stdin; a terminal would wait for typing |
| The PowerShell entry never reads stdin | `powershell.exe -File` cannot pass a pipe on unchanged (it is the Smart App Control fallback) |
| Enclosed references keep spaces | names with spaces are valid references; showing a little more is better than missing one |
| `/` and `=` in unenclosed references | the documentation's examples have them; without them nothing would match |
| Files: at most 8 MiB, unreadable ones quiet | templates are small; op itself reports a file it cannot read |
| `from` lists only the sources that gave a reference | a source without references is noise in a short head |
| 8 lines in the window, 1000 in the log, clips of 200 and 300 characters | the window stays short; the log keeps all in practice and stays bounded |
| `ui.refs` says `linked`, not the URL | the status needs no account ID |
| The launcher's link is a not focusable `Hyperlink` with a `WM_MOUSEACTIVATE` hook | a focusable one would make WPF call `SetFocus`, which activates the window |
| The hook is WPF's `Popup` filter, found by reflection | a script block cannot set the hook's `ref` parameter (seen on CI); compiled code (`Add-Type`) costs about a second per call and may be blocked where scripts run in constrained language; an `HwndSource` with `WS_EX_NOACTIVATE` did not show on the runner. Windows PowerShell 5.1 runs on .NET Framework 4.8, whose WPF no longer changes |
| The tests accept `view-item/?` as well as `view-item?` | the Windows shell adds the `/`; both name the same item |
| The PowerShell entry scans templates with one regular expression | a loop over characters is slow in PowerShell; the shared table holds it to the core's scanner |
| `op-reason-log` adds the count only when a file or stdin gave references | those in the arguments are in the command already |
| `op-reason.exe` opens on a thread of its own after `AllowSetForegroundWindow(ASFW_ANY)` | the window never waits for the shell, and 1Password may come to the front |
| The real click and the `onepassword:` handler are tested on CI only | they move the mouse and would replace the user's 1Password handler |
| The README animations stay as they are | the screenshots of the pull request show the block; the animations are a separate, larger change |
| No version change | releases are decided separately |

## 12. Checked with the real app, and not yet

- Checked: the six candidate URLs (§5.1), on the user's Windows with one account.
- Checked on CI (a Windows runner, a fake `op.exe` and a fake `onepassword:` handler): a real click on a link in
  `op-reason.exe`'s window and in the launcher's window hands the handler the item's URL, and neither window becomes
  the foreground window.
- Checked with a build of this change on the user's Windows (2026-11-17; the checks F1–F7 of
  `docs/manual-verification.md`; op-reason 0.2.0 at the pull request's head, 1Password CLI 2.41.1, the 1Password app
  8.12.44.33, Windows build 26300.9711, one account):
  - The first call listed both references of a template file, plain, with the hint (F1).
  - An approved call wrote `account.json` with an `id` of 26 uppercase letters and digits, a `host` under
    `1password.com` and nothing else: `op whoami --format json` names its fields `account_uuid` and `url` (F2).
  - The next call linked the ID reference and left the name reference plain, without the hint. A click opened the
    item through the shell (which hands on the `view-item/?` form, as seen on CI); the app came to the front and its
    prompt stayed in front of it, still answerable; op-reason's window did not become the foreground window (F3).
  - With 1Password locked, a click changed nothing: the app stayed locked, in the back, and the prompt could still
    be answered (F4).
  - `op-reason.exe` listed and linked the references of a template read from stdin (F5).
  - The launcher's window did as in F3; a log of the foreground window every 0.1 s showed that it never became the
    foreground window (F6).
  - `op-reason-log` added the count to the rows of F1, F3, F4 and F6, and `--json` had the list, for the calls from
    WSL and from Windows (F7).
- Not yet: more than one account (the machine has one).
