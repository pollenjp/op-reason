# op-reason: a modern look for the windows — design

- Date: 2026-11-26
- Status: approved (the user chose the direction of two drafts, the window of an `op` call and the SSH relay's window
  that asks, both in the dark theme, and asked for this change before the guide after Allow, which builds on it; the
  user also asked why the relay's window said that an answer "covers this call for 60 minutes", which reads like a
  permission for 60 minutes. This document records the design and the implementer's decisions)
- Builds on: `2026-11-15-windows-exe-design.md` (the window of `op-reason.exe`), `2026-11-15-language-toggle-design.md`
  (the switch, clicks that never activate the window), `2026-11-17-refs-in-window-design.md` (the references and their
  links), `2026-11-17-ssh-relay-design.md` §6 (the relay's window), `2026-11-18-pass-toasts-design.md` (the toasts, the
  screen of the windows), `2026-11-22-front-after-allow-design.md` (the window in front after Allow, the software
  renderer of the relay's windows, where a hidden prompt waits)

## 1. What and why

op-reason's windows are a light yellow box of "label: value" lines (`window.rs`, a copy of the launcher's WPF window).
They show everything, but nothing stands out: the reason, the line a person should read first, looks like the time;
the command is plain text; the person cannot tell where 1Password's prompt will come, or where it waits when it does
not come (it waited behind the icon in the tray, unseen, in the check that led to the guide after Allow). The user asked
for a richer, modern window: where the prompt will appear, and the reason and the actual command, shown better.

The user approved the direction of two drafts (dark theme, example values):

- **The window of an `op` call**: a header (an icon, the title, "op-reason · asking 1Password for approval", the time);
  the reason as the largest text; the caller and the repository with its branch as chips; the command in a monospace
  box with colors; the references as a list; a band with a small picture of the screen, which marks where 1Password's
  prompt usually comes (the middle of the screen) and the tray at the bottom right; the red footer with a shield; the
  language switch as a pill.
- **The SSH relay's window that asks**: the same header and reason; chips for the caller, WSL, herdr and the
  repository; the root command (`git push origin fix/login`) in the monospace box, ahead of the ssh command that the
  relay sees; what the signature is for and the host key in small text; a bar of the seconds left to answer; the band
  ("Allowing brings up 1Password's prompt: usually in the middle of the screen; if not, from the tray at the bottom
  right"); the buttons by color: Allow filled with the accent color, Deny outlined in red.

The relay's line "an answer covers this call for 60 minutes" read like a permission for 60 minutes (the user asked
whether 60 minutes needed a button of its own). An answer covers the call (its root process, the key and the reason)
until the root ends, 60 minutes at most; the button that allows for 60 minutes is **Allow this terminal for 60
minutes**, which the window offers only for a person's command. The line now says so, tied to the root command.

## 2. Goal, non-goals, anti-goals

Goal:

- The windows of `op-reason.exe` (the window of an `op` call, from both faces) and of the relay (the window that asks,
  `Phase::Ask`, and the one while 1Password asks, `Phase::Asking`) drawn in the drafts' direction (§4).
- Light and dark as Windows' app mode says (Settings → Personalization → Colors), also when it changes while a window
  is open.
- The relay's line about how long an answer lasts, tied to the root command (§4.8), in both languages and in the
  READMEs.
- The toasts of signatures that pass in the same colors and fonts; their layout, contents and times stay.
- The CI screenshots show both themes; the README images and texts show the new look.

Non-goals:

- The launcher's WPF window (the fallback where `op-reason.exe` cannot run): its look stays. The texts it shares with
  `op-reason.exe` (`Get-Texts`) stay word for word, and so does the status `ui` of the runner contract.
- The Windows notifications that come with a window (Windows draws them), the settings window (Windows' standard
  controls), the relay's icon and menu in the tray.
- What the windows do: when they open, what they ask, their answers, the records. The guide after Allow is the next
  change (`2026-11-26-prompt-guide-design.md`).
- A setting of op-reason's own for the theme.

Anti-goals (a failure even if it works):

- The window takes the focus, gets a default button or answers a key.
- An item, a button, a link, a note or a line of today's window is lost (§3).
- A new dependency (WebView2, a GUI toolkit), a bitmap, or a copy or screenshot of 1Password's screens in the
  repository.
- A relay window that no longer draws on Direct2D's software renderer.
- Text below WCAG AA contrast (4.5:1) on its background, in either theme.

## 3. What stays

- The answer is a click only (a press and a release on a button). The window opens without the focus
  (`WS_EX_NOACTIVATE`, `MA_NOACTIVATE`) and has no default button: a key typed elsewhere never reaches it.
- Only after a click on a button that allows does the relay's window come to the front, and it gives the front back
  when it closes (`2026-11-22-front-after-allow-design.md`).
- It opens at the top right of the work area of the screen of the foreground window, with that screen's DPI, 520 DIPs
  wide, 16 DIPs from the edges, topmost, out of the taskbar.
- The relay's windows draw into their device context with Direct2D's software renderer; the windows of `op` calls
  keep Direct2D's default.
- Every item of today's windows: the time, the caller (with its detail), herdr's workspace and pane, the WSL
  distribution, the place (repository, branch, directory), the reason (or "no reason given"), the command, the root,
  the destination and its host key (and both host keys of a forwarded request), what the signature is for, the key,
  the references (links to the 1Password app, "and N more", the line that says why there are no links), the notes
  (`shared-cleared`, `lock-skipped`), the orange line while other `op.exe` run, the red footer, the count of waiting
  calls, the last line while 1Password asks (where a prompt it did not show waits), the language switch.
- The buttons and their order: **Allow this call**, **Allow this terminal for N minutes** (only for a person's command),
  then **Deny** at the right end.
- The window class (`op-reason-approval`), the window text (the title, which the tests and the screenshots find), the
  close button of the caption (the relay: `dismissed`), and the points the tests click: 20 DIPs inside the client
  area's left and right edges and 26 above its bottom (the first and the last button), 26 DIPs left of its right edge
  and 21 above its bottom (日本語 of the switch in the window of an `op` call), 40 DIPs from its left edge (the links).
- Direct2D and DirectWrite, nothing else; the pictures are drawn with Direct2D's shapes and text.

## 4. The look

### 4.1 The frame

The caption stays (its close button, moving the window, the system menu, the window text), but it no longer shows the
title: the client area does. op-reason asks Windows, ignoring failures (an older Windows shows today's caption):

- not to draw the caption's text and icon (`SetWindowThemeAttribute`, `WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON`);
- in the dark theme, a dark caption and frame (`DWMWA_USE_IMMERSIVE_DARK_MODE`);
- the caption in the window's background color (`DWMWA_CAPTION_COLOR`, Windows 11), so that caption and client area
  are one surface;
- rounded corners (`DWMWA_WINDOW_CORNER_PREFERENCE`, `DWMWCP_ROUND`).

### 4.2 The window of an `op` call, top to bottom

| Part | What |
| --- | --- |
| Header | A 28 DIP rounded tile in the accent color with a lock; the title (14, bold); under it "op-reason · asking 1Password for approval" (11, secondary). At the right, the time (12, secondary) and under it the date (10, tertiary) |
| Reason | "Reason" (10, secondary), then the reason (17, bold), wrapped |
| Chips | One rounded chip each, wrapped onto more rows when they do not fit: the caller (a dot: purple for Claude Code, green for a person; `Claude Code (pid 42)` and its detail), herdr (`herdr · <label> · <pane>`), the repository and its branch (a small repository mark; `me/r · main`), the directory. A chip wider than the row wraps its text |
| Command | "Command" (10, secondary), then the command in a monospace box with colors (§4.6), wrapped |
| References | When the call reads `op://` references: the head (10, secondary), one per line in monospace with a small square before each, the links in the link color and underlined; "and N more" and the hint under them (secondary) |
| Notes | Each note with a round "!" mark in the caution color, its text (11, secondary) |
| Band | §4.9, the variant of an `op` call |
| Others | The orange line (11, bold) while other `op.exe` run (hidden otherwise, as today) |
| Footer | A shield and the footer (11, the critical color), wrapped; the language switch at the right of the row's bottom (§4.10) |

### 4.3 The relay's windows, top to bottom

| Part | Ask | Asking |
| --- | --- | --- |
| Header | A tile in the caution color with a key; the title; "op-reason's relay · per-call mode" (or "show-only mode") | the same, with the title "SSH key: 1Password is asking" |
| Reason | the reason, or "no reason given" in its place in the caution color (§4.5) | the same |
| Chips | the caller (purple: Claude Code, green: a person, gray: a program or unknown), `WSL · <distribution>`, `herdr · <label> · <pane>`, the repository and its branch, the directory | the same |
| Command | "Root command" and the root's command (§4.7) in the monospace box; under it, small and dim, the root process line as today (`bash (pid 31337) · bash -c -l "…"`) when the box shows less than it. A request without a root (forwarded, unknown, a WSL `ssh.exe` not started through `op-reason-ssh`): "Command" and the ssh command | the same |
| Facts | What the signature is for (12, primary, with a mark); one small line (10, tertiary) with the destination and its host key (or the forwarded hosts) and the key; one small monospace line with the ssh command (when the box shows the root) | the same |
| Countdown | A 4 DIP bar of the time left to answer (`--answer-wait`), in the caution color; under it "N s left to answer", how long the answer lasts (§4.8) and the count of waiting calls; the switch at the right | — |
| Band | §4.9, Ask | §4.9, Asking: what 1Password does now, and where a prompt it did not show waits |
| Footer | — | the shield and "Deny any 1Password prompt that comes without this window", the count of waiting calls; the switch at the right |
| Buttons | Allow this call (accent fill), Allow this terminal for N minutes (accent outline), Deny (red outline) at the right | — |

### 4.4 Colors

The tokens, for the dark theme (the drafts, close to Windows 11's dark colors) and the light theme (the same roles in
Windows 11's light colors):

| Token | Dark | Light | Used for |
| --- | --- | --- | --- |
| background | `#202020` | `#f9f9f9` | the window, the caption |
| text | `#ffffff` | `#1a1a1a` | the title, the reason, chips, the purpose |
| secondary | `#a0a0a0` | `#5d5d5d` | labels, the subtitle, the time, notes, "and N more" |
| tertiary | `#8a8a8a` | `#707070` | the date, the small facts, the root line |
| surface | `#2b2b2b` | `#ffffff` | chips, the switch, the toasts |
| line | `#3c3c3c` | `#e0e0e0` | the borders of chips, the switch, the toasts; the bar's track |
| code background / border | `#141414` / `#333333` | `#f0f0f0` / `#dcdcdc` | the command's box |
| code text | `#d4d4d4` | `#24292f` | the command's words |
| code path | `#e5b07b` | `#8a4b00` | file names, paths, references; the references' squares |
| accent | `#60cdff` | `#005fb8` | the tile of an `op` call, the program in the box, links, Allow, "the middle of the screen" |
| on accent | `#000000` | `#ffffff` | the text of Allow, the lock |
| caution | `#fce100` | `#9d5d00` | the relay's tile, "no reason given", the bar, "the tray", the notes' mark |
| critical | `#ff99a4` | `#c42b1c` | the footer, Deny |
| orange | `#ffb35c` | `#b35900` | the line while other `op.exe` run |
| success | `#6ccb5f` | `#0f7b0f` | a person's dot, the toasts' "Passed", check, bar and ring |
| Claude | `#c3a6ff` | `#7e57c2` | Claude Code's dot |
| band background / border | `#1b2a36` / `#2d4b63` | `#eaf3fb` / `#b3d3ee` | the band |
| band title / text | `#e6f3fb` / `#9cc9e8` | `#0b3550` / `#2d5f86` | the band's two lines |
| picture | screen `#15202a`, frame `#9cc9e8`, taskbar `#2d4b63`, stand `#4a6b85`, op-reason's window `#3c3c3c` | screen `#ffffff`, frame `#2d5f86`, taskbar `#b3d3ee`, stand `#8fb3d1`, op-reason's window `#d0d0d0` | the picture in the band |
| switch, shown | `#454545` | `#e6e6e6` | the part of the language shown |

### 4.5 Fonts and sizes

- Text in Segoe UI, as today (DirectWrite falls back to Yu Gothic UI and others for Japanese, with the user's locale).
- Monospace: the first of Cascadia Mono and Consolas that the system font collection has (Consolas is in every
  Windows op-reason runs on; Cascadia Mono comes with Windows 11).
- Sizes in DIPs: the title 14 bold, the subtitle 11, the time 12, the date 10, labels 10, the reason 17 bold, chips 11,
  the command 12, the references 11, the facts 12 and 10, the band 12 bold and 11, notes, the orange line, the footer
  and the switch 11, buttons 12 (Allow bold).
- "no reason given" (the relay) is the reason's text in the caution color, so that it stands where the reason would.

### 4.6 The command's colors

The command, as the window shows it today (masked, at most 300 characters), split into words the way it was joined
(spaces; a word in double quotes, `\"` inside), each word colored:

- the program (its file name) and, when the next word is a plain word (letters, digits, `-`, `_`), that word too: the
  accent color, bold (`op inject`, `git push`);
- a word that starts with `/`, `~`, `.` or `op://`, or holds `\` or a `.` between letters (a file name): the path
  color (`app.env.tpl`, `.env`, `C:\…\app.env.tpl`, `git@github.com`);
- the rest, options included: the code text color.

### 4.7 The relay's root command

The root's command line as the record keeps it (masked, at most 1000 characters: the command that Claude Code's
wrapper runs comes after some 300 characters of it). Only the root's own arguments are read as arguments: what a shell
runs is code, and code is never read as arguments (a word that starts with `-` inside it is not an option). A wrapper
comes off only when it matches exactly:

1. **A shell started to run one command**, read from the root's own arguments: `bash`, `sh`, `zsh`, `dash`, `fish` (also
   with `.exe`) with options first (a shell reads options until the first other word), only options that run nothing
   by themselves (groups of `-c`, `-l`, `-i`, `-e`, `-x`, `-u`; `--login`, `--noprofile`, `--norc`), `-c` among them,
   then the command string and nothing after it; `cmd.exe` with `/c` (after `/d`, `/s`, `/q` only); `pwsh` or
   `powershell` (also with `.exe`) with `-Command` (or `-c`) after `-NoProfile`, `-NonInteractive`, `-NoLogo` and
   `-ExecutionPolicy <policy>` (one of PowerShell's seven policies). What comes off is the shell and its options; what
   it runs is shown as it is, whatever it holds (no second shell inside it comes off).
2. **Claude Code's wrapper of its Bash tool**, inside a POSIX shell's command string: parts joined by ` && ` (outside
   quotes, as a POSIX shell reads them), every one of them a part that only prepares the shell, and exactly one
   `eval '<command>'` (optionally followed by ` \< /dev/null`): the command inside the quotes (`'\''` and `'"'"'` are
   quotes). The parts that prepare the shell, word for word: `source <snapshot>` (optionally followed by
   `2>/dev/null || true`), where the snapshot is a plain path (no spaces, quotes, `$`, redirections, globs or groups)
   ending in `.sh` under `/.claude/shell-snapshots/snapshot-`, or `/tmp/claude-shell-snapshot`; `shopt -u extglob
   2>/dev/null || true` (also after `\builtin`); `{ \builtin unalias -- '<name>'; \builtin unset -f -- '<name>'; }
   >/dev/null 2>&1 || true` (one name twice); and `pwd -P >| <plain path ending in -cwd>`. Seen on 2026-11-26 (the form
   of `tests/cases/roots.json`, `source … && eval '…'`, is an older one and matches too).

A part of any other kind keeps the shell's whole command string in the box. The record's line cut inside the eval's
quotes gives the command up to the cut and `…`; a line cut anywhere else, or whose quoting does not close, is shown as
the window showed it before (its first 200 characters).

Under the box, small: the root process (`<name> (pid <n>)`), and, when something came off, the root's line with the
box's command replaced by `…` (all of it, up to the record's 1000 characters). The box and that line show the whole
root line between them: a part of a wrapper that matched (a snapshot of an unexpected name, say) is still there to read.

### 4.8 How long an answer lasts

The relay's window that asks says, after the seconds left (Japanese in §5):

- "Allowing covers this `<name>` until it ends (at most N minutes)", where `<name>` is the root command's program (its
  file name without `.exe`), and for `git` its subcommand too (`git push`): the command the box shows;
- "Allowing covers this call until it ends (at most N minutes)" when the box shows no single program (a shell line with
  `&&`, `||`, `;` or `|`, an empty one);
- "This answer covers this signature only", as today, for a request that cannot be tied to a call.

On WSL the relay cannot see a root end, and an answer lasts the time limit; but no later request can come from that
process once it has ended (a new command is a new process, a call of its own), so "until it ends" holds there too.
**Allow this terminal for N minutes** keeps its label: it is the answer that lasts N minutes.

### 4.9 The band and its picture

A rounded box (the band's colors) with a picture of a screen at the left (64 × 40 DIPs: the screen, a taskbar along its
bottom, a stand) and two lines at its right, the first bold. In the picture: op-reason's window at the top right, the
place of 1Password's prompt in the middle, a dot for the tray at the bottom right of the taskbar.

| Variant | Picture | Lines |
| --- | --- | --- |
| `op` call | the prompt filled with the accent color | "1Password's prompt usually opens in the **middle of the screen**" / "If it does not, open it from 1Password in the **tray at the bottom right** (under ^)" |
| relay, Ask | op-reason's window in the caution color (this window), the prompt dashed and pale (to come) | "Allowing brings up 1Password's prompt" / "Usually in the **middle of the screen**; if not, from the **tray at the bottom right**" |
| relay, Asking | the prompt dashed, the tray dot larger | what 1Password does now (per-call after Allow: "Allowed here; 1Password itself may ask now"; slow: "1Password itself may ask now (to unlock it, for example)"; show-only: "Decide in 1Password's prompt") / today's last line, "If 1Password's prompt does not come: the 1Password icon at the right of the taskbar (under ^ when hidden) → SSH request waiting" |

The words in bold are drawn in the accent color (the middle) and the caution color (the tray). "Usually": where
1Password's prompt comes is not op-reason's to say; the checks with the real app note where it came.

### 4.10 The language switch

A pill: "English" and "日本語" side by side, the language shown on a darker (lighter) part and bold, the other one a
click target over its whole half. It sits at the right of the footer's bottom row (the window of an `op` call, the
relay's window while 1Password asks) or of the countdown's row (the relay's window that asks), so that its 日本語 half
covers the point 26 DIPs left of the client area's right edge and 21 above its bottom in the window of an `op` call.

### 4.11 The buttons

30 DIPs high, corners of 6 DIPs, 11 DIPs above the client area's bottom (the tests click 26 DIPs above it), labels 12
(Allow bold); at least 96 DIPs wide. Allow this call: the accent fill, its text on accent. Allow this terminal: the
surface with an accent border and accent text. Deny: the surface with a critical border and critical text, at the right
end. A pressed button gets a border 2 DIPs wide; its fill and its label stay, so that the label keeps its contrast.

### 4.12 The toasts

The cards keep their places, sizes, lines, opacity and times (`pass_toast.rs`). The face is the surface color with the
line color's border and a soft shadow; "Passed" and the check, the ×N tag, the bar of the time left and the ring are
in the success color (the tag on a tint of it); the caller in the text color; the command and its destination in the
monospace font; line 3 in the secondary color. `+N` is on a small tag of the surface color.

### 4.13 A window taller than the screen

The window is laid out for the work area of its screen. When it would be taller than the work area (less the margins):

1. the reason is drawn at 14 DIPs instead of 17;
2. then the band is left out (it says where the prompt comes, not what is asked), except while 1Password asks (its
   band holds what the window said before: the mode's words and where a prompt that was not shown waits);
3. then the command's box keeps as many lines as fit (at least two) and ends with `…`.

The buttons, the footer and the switch always stay inside the work area.

### 4.14 The theme

- Dark when `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme` is 0, light otherwise
  (1, missing or unreadable: Windows' default is light).
- A window reads it when it is made, and again on `WM_SETTINGCHANGE` with `ImmersiveColorSet` (Windows' message when
  the setting changes): its frame and colors change, its layout does not.
- The toasts read it when a card comes to an empty stack (as they read the animation setting).

## 5. Texts

New texts of `op-reason.exe`'s windows only (the launcher has no such parts; `Get-Texts` is unchanged):

| Key | English | Japanese |
| --- | --- | --- |
| subtitle of an `op` call | op-reason · asking 1Password for approval | op-reason · 1Password に承認を求めています |
| band of an `op` call | 1Password's prompt usually opens in the middle of the screen / If it does not, open it from 1Password in the tray at the bottom right (under ^) | 1Password の画面は、ふつう画面の中央に出ます / 出ないときは右下のトレイの 1Password (^ の中) から |
| relay subtitle | op-reason's relay · per-call mode / show-only mode | op-reason の中継 · 呼び出しごとに決めるモード / 表示だけのモード |
| root command | Root command | 起点のコマンド |
| countdown | {0} s left to answer | 答えを待つ残り {0} 秒 |
| how long | Allowing covers this {0} until it ends (at most {1} minutes) / Allowing covers this call until it ends (at most {1} minutes) | 許可は、この {0} が終わるまで効きます (長くても {1} 分) / 許可は、この呼び出しが終わるまで効きます (長くても {1} 分) |
| relay band, Ask | Allowing brings up 1Password's prompt / Usually in the middle of the screen; if not, from the tray at the bottom right | 許可すると 1Password の画面が出ます / ふつうは画面の中央。出ないときは右下のトレイから |
| relay band, Asking | Allowed here; 1Password itself may ask now / 1Password itself may ask now (to unlock it, for example) / Decide in 1Password's prompt | ここでは許可済み。いまは 1Password 自身が確認することがあります / いまは 1Password 自身が確認することがあります (ロックの解除など) / 決めるのは 1Password の画面です |
| relay footer, Asking | Deny any 1Password prompt that comes without this window | この窓が出ていない 1Password の確認は拒否してください |

The texts the relay's footer had ("per-call mode · an answer covers this call for 60 minutes" and the three footers
while 1Password asks) are replaced by the subtitle, the line of §4.8, the band and the footer above. The ssh command's
small line keeps the label "Command"; the destination's line keeps "Destination" and "Key".

## 6. Approaches

| Approach | Trade-off |
| --- | --- |
| **Draw the drafts with Direct2D and DirectWrite (chosen)** | everything in op-reason's own code; the shapes are a few rectangles, lines and text layouts |
| WebView2 with HTML and CSS | the look is easy, but the approval gate gets a browser engine, a runtime to install or bundle, and its attack surface (the conductor's decision: no) |
| A GUI toolkit (WinUI, a Rust UI crate) | new dependencies and their updates for one window; the focus rules (`WS_EX_NOACTIVATE`) would have to be won again |

Within the chosen approach:

| Decision | Chosen | Not chosen, and why |
| --- | --- | --- |
| The frame | Windows' caption, without its text and in the background color | A frame of op-reason's own: the close button, moving and the system menu would have to be drawn and handled again |
| The width | 520 DIPs, as today | The drafts' 440: long commands and paths (the template of the screenshots) would wrap more |
| The theme | Windows' app mode | Always dark (the drafts): a light desktop would get a dark window; a setting of its own: nobody asked |
| The relay's texts | A structured view (`View`) built from the facts, rendered by the window | Today's "label: value" lines: the window could not tell the reason from the time |
| The `op` call's texts | The launcher's lines kept (the status `ui`), and a `View` built next to them | Changing the lines: the runner contract and the launcher's table would change |
| The root command | The wrappers taken off only when they match exactly, the whole line kept small | Always the whole line (`/bin/bash -c -l "source … && eval '…' …"`): the command a person reads drowns; taking off more: a wrong guess would hide a part of what runs |

## 7. Code

- `crates/op-reason-win/src/theme.rs` (portable): the theme, the two palettes of §4.4, the contrast of two colors.
- `crates/op-reason-win/src/look.rs` (portable): the `View` that the window draws (header, reason, chips, command, facts,
  notes, countdown, band, footer), the colored words of a command (§4.6), the root command (§4.7), the name of §4.8,
  the marked words of the band's lines, the rows of chips, the pill of the switch and its click target.
- `ui.rs` and `relay_ui.rs` build a `View` in each `Texts` (both languages, as today). The window of an `op` call keeps
  `lines`, `refs`, `footer` and `others` (the status `ui`); the relay's `Texts` carry their facts in the `View` only.
- `src/win/window.rs` draws the `View`: the layout in DIPs, then one Direct2D frame (both surfaces as today). The links,
  the switch and the buttons are hit as today (the switch: its pill).
- `src/win/theme.rs`: the registry value, and the frame's attributes (`DwmSetWindowAttribute`,
  `SetWindowThemeAttribute`). The `windows` crate gets the features `Win32_Graphics_Dwm` and `Win32_System_Registry`
  (no new crate).
- `src/win/pass_window.rs`: the palette of the theme.

## 8. Tests

- Linux (`cargo test` in `crates/op-reason-win`): every text color on its background meets 4.5:1 in both palettes; the
  colored words of the commands of the screenshots and the drafts; the root commands of `tests/cases/roots.json` and of
  the screenshots, a shell line with `&&`, an unbalanced quote, a cut line, a wrapper with more after it; the name of
  §4.8 (`git push`, `ssh-add`, none); the band's marks; the rows of chips; the switch's click target; the `View` of an
  `op` call (references, notes) and of the relay in both phases and both languages (no reason, a forwarded request, the
  terminal's button, waiting calls, show-only, a slow answer); the texts the launcher shares are unchanged.
- Windows CI (`cargo test`): the tests that click (Allow, Deny, the switch, a link) pass with the new layout; the
  window's text is still the title.
- Screenshots (`windows-exe-shots.ps1`): the window of an `op` call (with the references and their links), the relay's
  window that asks (Claude Code; a person's command, three buttons), while 1Password asks, and the toasts, in the dark
  theme and in the light theme (the script sets `AppsUseLightTheme` for the scenes and puts it back).

## 9. Documents

- README.md and README.ja.md: the window's description (the reason first, chips, the command's box, the band, the
  theme), the line of §4.8, the toasts' face; the animations (`docs/images/make_readme_svgs.py`) draw the new look in
  the dark theme, and its copies of the Rust texts follow them; the HTML page of the relay's scene
  (`docs/html/relay-screens.html`) too.
- `docs/architecture/README.md`: the window and the toasts, the theme.
- `docs/manual-verification.md` and `.ja.md`: a check of both themes and of a theme changed while a window is open.
- `CHANGELOG.md`: Unreleased.

## 10. The builder's questions, and the answers (the implementer's decisions)

| Question | Decision | Why |
| --- | --- | --- |
| Is "now" under the time (the drafts) kept? | The date instead | A window of `op run` stays open while its command runs; "now" would go stale, and the date does not |
| Where do the directory and the caller's detail go? | The directory is a chip of its own; the detail goes in the caller's chip | Every item of today stays (§3); the drafts showed the repository chip only |
| What happens to a chip wider than the row? | It takes the row and wraps its text | Clipping would hide a part of an item (the caller's process chain can be long) |
| Which color for "no reason given"? | The caution color | A person's own `ssh` has no reason most of the time: red would cry wolf |
| What is the middle button? | The accent outline | Two filled accent buttons side by side would not tell which is the plain Allow |
| Does the relay's window keep a red footer while it asks? | Not in Ask (as the draft); in Asking, the line to deny prompts that come without the window | Today's Ask footer told the mode and the time, which now have places of their own |
| The ssh command: fold it or make it small? | Small (one monospace line) | A fold is a click target more in a window that answers clicks |
| How does the countdown move? | A timer of the window redraws it every 250 ms from the deadline the relay set | The bar and the seconds stay right without the relay's thread doing more |
| The README animations: which theme? | Dark, the drafts' | One look in the README; the text says that the window follows Windows' setting |
| Claude Code's wrapper today has more parts (`2>/dev/null \|\| true`, `shopt`, `unalias`), quotes with `'"'"'`, and puts the eval after some 300 characters, past the window's 200: does rule 2 still find the command? | Rule 2 lists those parts word for word (§4.7), and reads the record's line of 1000 characters | The command a person reads is the reason for the box; with the old rule no call of Claude Code would show it |
| Then what does the small line under the box show? | The root process, and the root's line with the box's command as `…`, all of it (decided after the review below) | The box and that line show the whole line between them |
| What when the window is taller than the work area? | The steps of §4.13 | Today's window could grow out of the screen too; the new one is taller, and its buttons must stay reachable |
| The drafts are not in the repository: what is the source? | This document (§1, §4); the drafts' commands are written out in §4.6 and §4.7 and in the tests | The drafts carried example values; the repository keeps its own examples |
| A pressed button with an overlay drops the label below 4.5:1 | A border 2 DIPs wide instead (§4.11) | The contrast rule of §2 holds in every state |

## 11. Left to the implementer

The exact paddings and gaps around the parts (the points of §3 must hold), the marks (lock, key, repository, shield,
"!"), the tests' helpers.

## 12. The review before the pull request

A fresh reviewer read the code against this document. What it found, and what changed:

| Finding | Change |
| --- | --- |
| The command strings that a shell runs were read as arguments: a word like `-x;evil` inside one was taken for an option, and a second `bash -c` inside the eval came off, so that the box could show `git push origin fix/login` while what followed it ran unseen | Only the root's own arguments are read as arguments; what a shell runs is shown as it is; a shell's options must come first and be ones that run nothing (§4.7) |
| A part of the wrapper such as `source <(curl …)` or `source ./evil.sh` counted as preparing the shell, and the small line (the first 200 characters) could not show it | `source` takes only a plain path of Claude Code's snapshots; the small line shows the whole rest of the root's line, so a matching part that is unexpected is still there to read (§4.7) |
| Splitting at ` && ` compared at a byte inside a character that is not ASCII (a home folder in Japanese, the `…` of a cut line) and panicked, which failed the request | The split compares bytes; a test cuts lines with such characters at every character and calls every function of the look |
| The answer wait started before the window was up | It starts when the window has been drawn |
| On a small screen, the band was left out while 1Password asks too, and with it the window's old last line | That band stays (§4.13) |
| When nothing came off, the root's name and PID were gone | The small line shows the root process always |
