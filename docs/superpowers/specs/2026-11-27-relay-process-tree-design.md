# op-reason: the relay's process tree, and one ssh at a time for Claude Code's PowerShell tool — design

- Date: 2026-11-27
- Status: approved. The user watched the relay's window for a call of Claude Code's PowerShell tool (0.8.1,
  `2026-11-26-modern-window-design.md` §13) and asked for a process tree, with the command still in front. Of two
  drafts, the user chose the one that puts the box on the tree's leaf. The user also asked that the relay ask for each
  ssh of a call of that tool, with a second button that allows the whole call; the conductor checked with the user how
  the three buttons read. This document records the design and the implementer's decisions.
- Builds on: `2026-11-17-ssh-relay-design.md` §5 (per-call mode: the mark of a call, the answers the relay remembers,
  the queue of windows) and §9 (the records); `2026-11-18-pass-toasts-design.md` §4.3 (the toast of a signature that
  passes); `2026-11-26-modern-window-design.md` §4.3, §4.7, §4.8, §4.13 and §13 (the relay's window, its root command,
  how long an answer lasts, Claude Code's PowerShell tool).

## 1. What and why

The relay's window shows the root command in a box, and under it, small, the root process with the rest of the root's
line. For a call of Claude Code's PowerShell tool, that small line reads `cmd.exe (pid 2024) · Claude Code's PowerShell
tool`. The user asked for a tree of processes instead. The command keeps the emphasis, and it shows at a glance what
`cmd.exe` is to Claude Code: its child, the tool's launcher.

The user also asked what happens to a second ssh in the same call, for example one that the tool's script runs after
the first. An answer covers its call: the root process, the key and the reason (`Mark` in `ssh/decide.rs`). So the
second ssh signs without a window, and a toast tells of it only afterwards. For the PowerShell tool, the script never
reaches the window, so nobody can know of the second ssh before the click. The user chose that the relay ask for each
ssh of such a call. A second button allows the whole call, as the one button does today. It does not allow every call
of Claude Code for 60 minutes.

## 2. Goal, non-goals, anti-goals

Goal:

- A tree in the relay's windows: the window that asks (`Phase::Ask`), and the one while 1Password asks (`Asking`, with
  its forms after the click, `Seen` and `Hidden`).
  - The tree runs from the caller (Claude Code, a person's shell or a program) down to the client (`ssh.exe`). It
    takes the place of the command section.
  - The box of the command sits at the row of the process whose text it shows (§3).
- In the window that asks, each answer's reach is framed on the tree (§3.4).
- Each ssh asks on its own for a call of Claude Code's PowerShell tool (rule 3 of §4.7 of the modern-window design).
  "Allow this ssh" is the default and covers that ssh until it ends. "Allow this call" covers the call, as today (§4).
- The pass toasts and the `ssh-sign` records tell which answer let a signature through (§4.5).
- These follow (§8, §9): the tests on Linux and Windows, the screenshots, the READMEs, the guide for agents, the
  architecture document, the manual checks and the CHANGELOG.

Non-goals:

- The window of an `op` call (the other face of `op-reason.exe`) and the launcher's WPF window.
- The command lines of the processes between the root and the client. They do not travel to the window, for the
  reason of approach B in §13 of the modern-window design: another process's line would have to travel, and the script
  stays unseen anyway.
- What travels from WSL: `OPREASON_SSH_CALL` (v1) and the payload of `op` calls stay as they are, and the tree shows
  what they carry.
- The answers of other calls. Claude Code's Bash tool, a person's command and a program keep "Allow this call" and
  "Allow this terminal for N minutes": their box shows every word that runs.
- A setting to turn the questions for each ssh off.

Anti-goals (a failure even if it works):

- A word that today's window shows (the root command, the rest of the root's line, the ssh command) is lost.
- A frame that shows less than its answer covers, or an answer that covers more than its frame shows.
- After "Allow this ssh", another ssh of the same call signs without a window.
- The window takes the focus, gets a default button or answers a key; a button that another process can press by a
  message.
- Text below 4.5:1 on its background, in either theme.

## 3. The process tree

### 3.1 The rows

Parent above child, one row per process, each child indented under its parent with an elbow.

| Row | Which process | Look |
| --- | --- | --- |
| Head | The caller: Claude Code's `claude.exe` (the root's parent); a person's interactive shell; for a program (rule `parent`), the root itself | The caller's dot (purple, green, gray); the caller's name in bold (`Claude Code`, `Human (terminal)`, or the program's image); then the image and PID in the tertiary color (`claude.exe · pid 1000`) |
| Root | The root of the call (`2026-11-17-ssh-relay-design.md` §5.2) | The image in bold, `pid N` in the tertiary color; for rule 3, a pill "Claude Code's PowerShell tool"; then the mark "Root", outlined in the caution color |
| Between | The processes between the root and the client, the root's child first | The image in the secondary color, `pid N` in the tertiary color |
| Unseen | A WSL call: the processes between the root and `ssh.exe` are on Linux, where the relay cannot look | `…` and "in WSL, not seen", in the tertiary color |
| Client | The process that connected to the relay's pipe (`ssh.exe`, `ssh-keygen.exe`, …): the leaf | The image in bold, `pid N` in the tertiary color |

Which rows, by the call:

| Call | Rows |
| --- | --- |
| Windows, rule `claude` | `claude.exe` → root → between → client |
| Windows, rule `shell` | the shell → root → between → client |
| Windows, rule `parent` | the root, as the head → client |
| WSL (`OPREASON_SSH_CALL`) | the head from the call's facts (Claude Code with `caller.claude_pid`, the shell, or the root) → root → unseen → client (`ssh.exe`, a Windows PID) |
| No root (forwarded, a WSL `ssh.exe` without `op-reason-ssh`, a client that could not be read) | no tree: "Command" and the ssh command in the box, as today |

A root that is the client itself (a chain of one process) gives one row, with the box. The caller's chip stays in the
row of chips.

### 3.2 The box and the small lines

The box keeps its look (§4.6 and §4.7 of the modern-window design) and goes under the row of the process whose text it
shows.

| Call | Under the root's row | Under the client's row |
| --- | --- | --- |
| Rule 3, Claude Code's PowerShell tool | nothing: the pill says what the root is, and the launcher's line stays in the record | the box, with the ssh command |
| Any other call with a root | the box, with the root command (§4.7); under it, small, the rest of the root's line with the box's command as `…`, when something came off | the ssh command, small (monospace, the tertiary color), wrapped |

Today's small line under the box splits: the root process goes to the root's row, the rest of the line stays under the
box. The ssh command's small line among the facts (`Command: …`) goes to the client's row, without its label; when the
root is the client itself (`ssh` typed in a terminal), there is no client's row, and it goes under the root's box (that
box shows only the first 200 characters of the line). The facts keep what the signature is for and the line with the
destination and the key.

### 3.3 A deep tree

Up to four processes between are shown. With more, the window shows the first one, a row "… N more processes", and the
last two. A window that would be taller than its work area folds them all into that row (§3.6).

### 3.4 The frames: what each answer covers

In the window that asks, each button that allows draws its reach on the tree. The reach is a rounded frame from its
first row to the bottom of its last row (with that row's box or small line), with its name at its top right.

| Call | Buttons | Frames |
| --- | --- | --- |
| Rule 3 (§4) | Allow this ssh, Allow this call, Deny | "this ssh": the client's row and its box, solid; "this call": the root and below, dashed |
| Claude Code's Bash tool, a program | Allow this call, Deny | "this call": the root and below, solid |
| A person's command (the terminal's answer offered) | Allow this call, Allow this terminal for N minutes, Deny | "this call": the root and below, solid; "this terminal": the shell and below, dashed |
| An answer that covers its signature only (`single`, forwarded, unbound) | Allow this call, Deny | none |

- The filled button's frame is solid; the outlined button's frame is dashed. Neither is filled (§11).
- An inner frame ends 8 DIPs inside the outer one's right edge.
- The frames take no room of their own: they are drawn in the gaps around the rows. The boxes under the rows end 14
  DIPs before the right edge, and a row where a frame starts leaves room for the frame's name.
- The windows without buttons (`Asking`, `Seen`, `Hidden`) show the tree without frames.

### 3.5 The look

- The section's label: "Process tree" (プロセスの木), 10, secondary, in the place of "Root command" and "Command".
- The rows:
  - Each level indents 16 DIPs, and a row is 20 DIPs high.
  - The names are 12; the PID, the pill and the mark are 11.
  - An elbow runs from below the parent's dot or name down to the child's middle, then right to its name.
  - The elbows are 1.5 DIPs wide, in a new token `tree`: dark `#5d5d5d`, light `#b4b4b4`. They are not text, so no
    contrast rule applies.
- The pill: the surface color with the line's border, its text in the Claude color.
- The mark "Root": the caution color's border, its text in the caution color, bold.
- The frames: the accent color, 1.5 DIPs when solid, 1 DIP dashed; their names 10, in the accent color, bold for the
  solid one, inside the frame at its top right.
- A box under a row starts at that row's text and ends inside the frames.

### 3.6 A window taller than the screen

The steps of §4.13 of the modern-window design, with one more before the box is cut:

1. the reason at 14 DIPs instead of 17;
2. the band left out (except while 1Password asks);
3. the processes between the root and the client folded into one row ("… N more processes");
4. no tree: the command section as it was before the tree (the box, the root process and the rest of its line under
   it, the ssh command's small line among the facts), without frames; it is no taller than the window of 0.8.1;
5. the box keeps as many lines as fit (at least two) and ends with `…`.

## 4. One ssh at a time: Claude Code's PowerShell tool

### 4.1 Which requests

In per-call mode, a request asks for its ssh when all of these hold:

- Its call's root is the launcher of Claude Code's PowerShell tool (rule 3 of §4.7). That is, the call is Claude
  Code's, its root's image is System32's `cmd.exe`, and the root's own arguments are the launcher's.
- Its answer can be tied to its call: not `single`, forwarded or unbound.

The relay marks such a request on the window's facts (`per_ssh`).

### 4.2 The answers

| Button | Answer (record) | Covers |
| --- | --- | --- |
| Allow this ssh (accent fill, the first) | `allow-ssh` | the requests of this call (root, key, reason) from this client process (its PID and start time), until the process ends or the time limit passes |
| Allow this call (accent outline) | `allow` | the call, as today: until its root ends or the time limit passes |
| Deny (red outline, at the right end) | `deny` | the call, as today |

- "this ssh" is the process that connected to the relay's pipe, the leaf of the tree, whatever its name.
- When the client's start time cannot be read, "Allow this ssh" lets this request go and remembers nothing (a PID
  without its start time could be reused). The next request asks again.
- Deny still covers the call. A script that tries again after a denial gets no new window: its call was refused, and
  the guide for agents says not to run it again.
- The book (`Book`) keeps the answers for a process next to those for calls and terminals. It looks them up after the
  call's and before the terminal's. It forgets them at the time limit or when the process ends, in a lookup and in the
  sweep.

### 4.3 The queue

The queue is unchanged, but the book comes first:

- A request that an answer covers goes at once, before it looks at the queue: an answer for its own ssh does not wait
  for the window of another ssh of the call.
- A request of a call whose window asks waits for that window (`Join::Follow`), then looks again.
- An answer for the call covers it.
- After "Allow this ssh", a request from another process finds no answer and takes a ticket of its own, so its window
  comes next.
- A window that ended without an answer (timed out, closed) fails the requests that waited for it, as today.

### 4.4 The window's words

- The title: "SSH key: allow this ssh?" (「SSH の鍵: この ssh を許可しますか」). It is the window text too, which the tests
  and the screenshots find.
- The line after the seconds left: "Allowing covers the frame you choose (this ssh or this call) until it ends (at most
  N minutes)". In Japanese: 「許可は、選んだ枠 (この ssh か、この呼び出し) が終わるまで効きます (長くても N 分)」. It
  replaces the line of §4.8 of the modern-window design for these windows.
- The buttons as in §4.2. In every window, the first button (unless it is Deny) is filled with the accent color, with
  a bold label. The other buttons that allow are outlined.

### 4.5 Toasts and records

- Line 3 of the pass toast says "allowed for this ssh" (「この ssh の許可」), next to "allowed for this call" and
  "allowed for this terminal".
- In the `ssh-sign` records:
  - The click's `answer` gains a word, `allow-ssh`.
  - A record that an earlier answer covered (it has `decision`) gets `by`: that answer's word (`allow`, `allow-ssh`,
    `allow-terminal`, `deny`).
- `op-reason --log` puts "(this ssh)" after a result whose click or covering answer is `allow-ssh`. "(terminal)" goes
  after one whose covering answer is the terminal's, too.

## 5. Texts

| Key | English | Japanese |
| --- | --- | --- |
| tree label | Process tree | プロセスの木 |
| root mark | Root | 起点 |
| unseen | … · in WSL, not seen | … · WSL の中は見えない |
| more | … · {0} more processes | … · ほかに {0} 個のプロセス |
| frames | this ssh / this call / this terminal | この ssh / この呼び出し / このターミナル |
| title of §4 | SSH key: allow this ssh? | SSH の鍵: この ssh を許可しますか |
| button of §4 | Allow this ssh | この ssh を許可 |
| line of §4.4 | Allowing covers the frame you choose (this ssh or this call) until it ends (at most {1} minutes) | 許可は、選んだ枠 (この ssh か、この呼び出し) が終わるまで効きます (長くても {1} 分) |
| pass toast | allowed for this ssh | この ssh の許可 |
| `--log` | (this ssh) | (この ssh) |

The pill is the existing "Claude Code's PowerShell tool" (「Claude Code の PowerShell」). The head's names are the
existing "Claude Code" and "Human (terminal)" (「人 (端末)」).

## 6. Approaches

| The tree | Trade-off |
| --- | --- |
| **A: the box on the row of its process (the user's choice)** | the command sits where it runs, and the tree reads top down; for rule 3 the box is on the leaf |
| B: the box where it is today, the tree under it | the box stays first, but the root shows twice, and for rule 3 the box is far from the leaf it belongs to |

| The second ssh of a call | Trade-off |
| --- | --- |
| **The relay asks for each ssh of a call of the PowerShell tool, with a button for the call (the user's choice)** | the relay knows the process of each request (`client_pid` of the pipe, its start time from the chain); a call that runs many ssh asks many times, unless the person allows the call |
| A rule in the guide for agents: one ssh per tool call | rests on the agent; nothing stops a script that runs two |
| As today | the second ssh signs, seen only in a toast after |

Within the chosen approaches:

| Decision | Chosen | Not chosen, and why |
| --- | --- | --- |
| The frames | drawn in the gaps, taking no room | Room of their own: the window would move when the buttons go after the click |
| Frames while 1Password asks | none | They explain the buttons, which are gone |
| Deny for rule 3 | covers the call, as today | Covering one ssh: after a denial the script would bring window after window, and the guide tells the agent to stop and ask |
| The WSL host (`wsl.exe`, `wslhost.exe`) | not a row | It starts Windows programs for WSL; the chip says WSL, and the unseen row says where the chain goes on |
| Requests without a root | no tree, the box as today | There is no call to draw: the answer covers the signature only |
| The second button's word | `allow`, as today's Allow this call | It covers what it covered; the readers of `allow` stay right |
| The title of rule 3 windows | "allow this ssh?" | The question the default button answers |

## 7. Code

- `crates/op-reason-core/src/ssh/decide.rs`:
  - `Answer::AllowSsh` (`allow-ssh`).
  - `Book` keeps a map from the call's mark and a client process (`ProcId`) to a decision.
  - `lookup` and `remember` take the client (`Option<&ProcId>`); `sweep` and `len` include the new map.
- `crates/op-reason-core/src/ssh/record.rs`: `Sign::by`.
- `crates/op-reason-core/src/logview.rs`: the labels of §4.5.
- `crates/op-reason-win/src/relay_ui.rs`:
  - `Facts::tree` (the processes of §3.1: the head, the processes between, whether some are unseen, the client) and
    `Facts::per_ssh`.
  - The texts of §5; the view's tree and frames, the title, the buttons and the line of §4.4; `pass_texts` for
    `AllowSsh`.
- `crates/op-reason-win/src/look.rs` (portable): the tree of the view (rows, the row of the box, the small lines, the
  frames) and the fold of §3.3 and §3.6.
- `crates/op-reason-win/src/ui.rs`: `Button::AllowSsh`.
- `crates/op-reason-win/src/theme.rs`: the token `tree`.
- `crates/op-reason-win/src/win/window.rs`:
  - Draws the tree, the box at its row, the small lines and the frames.
  - The fit step of §3.6.
  - The buttons styled by their place (the first that allows is filled).
- `crates/op-reason-win/src/win/relay/conn.rs`:
  - The tree from the chain (`identify`) and from a WSL call (`from_wsl`).
  - The client's `ProcId`, `per_ssh`, and `by` in the records.
- `crates/op-reason-win/src/win/relay/ask.rs`: the client to the book; `Button::AllowSsh` to `Answer::AllowSsh`.
- `.github/scripts/windows-exe-shots.ps1`: the scenes of §8.
- `docs/images/make_readme_svgs.py` and `docs/html/relay-screens.html`: the relay's scene with the tree.

## 8. Tests

- Linux (`cargo test`):
  - The book: an answer for a process covers that process only, ends with it or at the time limit, and is swept. An
    answer for the call still covers every process; a denial covers the call; requests that cannot be tied to a call
    are never remembered.
  - The records' `by`; the labels of `--log`.
  - The views in both languages:
    - Rule 3: its title, the three buttons, the frames, the line of §4.4.
    - Claude Code's Bash tool: the box at the root with the rest of the line, the ssh command at the leaf.
    - A person's command with the terminal's frame; a program as the head; WSL with the unseen row; no root.
    - A deep tree, folded and not; the windows without buttons, which have no frames.
  - The pass toast of `AllowSsh`; the new inks on their backgrounds (4.5:1); `make_readme_svgs.py --check`.
- Windows CI (`cargo test`), the launcher of Claude Code's PowerShell tool running two `ssh.exe` one after the other:
  - "Allow this ssh" in each of the two windows: two `allowed` records with `allow-ssh`.
  - "Allow this call" in the first window: the second ssh passes (`passed`, `by` `allow`).
  - One `ssh.exe` that signs twice after "Allow this ssh": the second signature passes (`passed`, `by` `allow-ssh`).
  - The existing test of the launcher, with the new title.
- Screenshots: the window of rule 3 (Japanese, and English in the light theme) shows the tree, the frames and the
  three buttons. The scenes of Claude Code's Bash tool and of a person's command show their trees.

## 9. Documents

- `README.md` and `README.ja.md`:
  - The relay's window: the tree, the frames.
  - The answers for Claude Code's PowerShell tool.
  - The records' `by`.
- `skill/SKILL.md`: "One answer covers one call", and through Claude Code's PowerShell tool, each ssh asks unless the
  person allows the call.
- `docs/architecture/README.md`: `ssh-sign` (`answer`, `by`), the book, the window.
- `docs/manual-verification.md` and `.ja.md`: checks of the tree (Claude Code's Bash and PowerShell tools, a person's
  terminal, WSL) and of the two answers for the PowerShell tool.
- `CHANGELOG.md`: Unreleased.
- The README's animation and the HTML page of the relay's scene: the tree.

## 10. What changes for other programs

- Unchanged: `OPREASON_SSH_CALL` (v1), the payload of `op` calls, the runner's contract, the status `ui`.
- `ssh-sign` keeps `v` 1, with a new optional field (`by`) and a new word for `answer` (`allow-ssh`).
  - The readers in this repository change with it: `op-reason --log` and `docs/architecture/README.md`.
  - The dotfiles read none of these words (checked on 2026-11-27). Their hook `ssh-agent-notice.sh` says that the
    person allows the call in op-reason's window, which stays true, and it reads no records.
- The guide for agents (`op-reason --skill`): its line about how much one answer covers changes for the PowerShell
  tool.

## 11. The builder's questions, and the answers (the implementer's decisions)

| Question | Decision | Why |
| --- | --- | --- |
| Which name shows for a WSL call of Claude Code, whose payload carries the PID only? | `Claude Code` and `pid N` (no image) | The payload has no image for it; the PID is what the record has |
| Does "this ssh" say so when the client is not `ssh.exe` (`ssh-keygen.exe`)? | Yes; the leaf's row names the image | The user chose these words; the frame shows which process it is |
| How many processes between before folding? | Four; then the first, "… N more processes", the last two | The root's child says what the root ran, and the client's parent (often `git.exe`) what started ssh |
| The draft fills the solid frame with the accent color at 7%. The tertiary text inside it (the PIDs, the small lines) drops below 4.5:1 there: 4.1 in the dark theme, 4.2 in the light one; the light caution of the mark to 4.49. What gives way? (the builder's one question) | Neither frame is filled. The solid frame is told from the dashed one by its line and its bold name | The anti-goal of contrast holds in every state. A tint light enough for the tertiary text is invisible in the dark theme, and darker inks only inside the frame would change the tree's tones |

## 12. Left to the implementer

The exact paddings, the elbow's path, the sizes of the pill and the mark, the names of the tests' helpers and how the
scenes of the screenshots are arranged.

## 13. The review before the pull request

A fresh reviewer read the branch against this document. It found no way for a signature to pass without the window
that this document asks for. What it found, and what changed:

| Finding | Change |
| --- | --- |
| When the root is the client itself (`ssh` typed in a terminal), the tree has no client's row, so the ssh command's small line was gone; the root's box shows only the first 200 characters of the line, and the 201st to the 300th were lost | The small line goes under the root's box then (§3.2); a test with a line of more than 200 characters |
| The tree made the window some 70 DIPs taller, and nothing took that back on a small screen: the window of the PowerShell tool has one process between, so folding saved nothing, and the box has one line to cut. On 1366 × 768 at 150 % the buttons would go under the taskbar, where the window of 0.8.1 fit | A step before the box is cut drops the tree for the command section as it was before (§3.6). The view keeps that section as it was (the box, its small line, the ssh command among the facts) and the tree carries its own box; the window leaves the facts' ssh line out when it draws the tree |
| A request that "Allow this ssh" covered waited behind the window of another ssh of the same call (the queue was looked at before the book), and failed with it when that window timed out or was closed | The book is looked up first (§4.3) |
| The tests of the view without a tree tested a path that a request with a root never took | They test the command section that a short screen shows now; the tests of the tree check its own box and its small line |
| S40 ran a plain `ssh` in WSL, and expected the box to show `ssh -T git@github.com`, where the reason's assignment shows too | S40 runs `op-reason-ssh`, as S3 does, and expects the command as Claude Code ran it |
| The READMEs said that more than four processes fold into one row, called "Allow this ssh" the default next to "no default button", and said that `by` comes with a signature "let through" while it comes with a refused one too | The words say what the window and the records do |
