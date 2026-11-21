# op-reason: the SSH relay's window comes to the front after Allow — design

- Date: 2026-11-22
- Status: approved (checks with the real app showed that 1Password keeps its prompt behind its icon for a relay
  started from a scheduled task; of the ways out, the user chose to bring op-reason's window to the front at the click
  on Allow, to be built and checked with the real app, as the better experience than approving from the tray each time;
  this document records the research behind it, the design and the implementer's decisions). §11–§13 hold the checks
  with the real app of this pull request: the first build's window hung (§12); the build that draws without the GPU
  brought 1Password's prompt at once (§13).
- Builds on: `2026-11-17-ssh-relay-design.md` (§5.5 the windows one at a time, §6 the window, §8 the relay process,
  §15 the checks with the real app), `2026-11-15-language-toggle-design.md` (clicks that never activate the window),
  `2026-11-17-refs-in-window-design.md` §5.4 (handing the right to the foreground on after a click) and
  `2026-11-18-pass-toasts-design.md` §5 (the screen of the windows).

## 1. What and why

The relay is meant to run from a scheduled task at logon (README, "Starting it"). Checks with the real app on the
author's machine (2026-11-21 and 2026-11-22; op-reason 0.5.0, 1Password at "application and terminal session"):

- **A relay started from a scheduled task** (its parent the Task Scheduler's host, `svchost.exe` in session 0): after
  **Allow this call**, 1Password showed no prompt; after 59.7 seconds it closed the request without an answer, and ssh
  failed with `agent refused operation`. Twice, and the same for a relay that Explorer started.
- **The prompt was hidden, not refused.** In the next check the 1Password icon at the right of the taskbar had a dot,
  and its menu had "SSH request waiting" (in Japanese, SSHリクエストを待機中……). Opened from there, the prompt read
  "Allow op-reason to use SSH key" (in Japanese: 許可 op-reason 使用 SSHキー): 1Password took op-reason for the
  application that asks. Once approved, the signature went through. 1Password's log said that it notified the user
  through the tray icon that a background prompt was waiting.
- Once approved, the next calls through the same relay passed without a prompt (5.6 seconds, as before: until 1Password
  locks). A relay started again (a new process) was asked again, and again only through the tray.
- A relay started in a terminal program got its prompt at once, naming the terminal program (S1, S12 and S18 of
  `docs/manual-verification.md`, 2026-11-18). A relay whose parent had ended got no answer (S15, and the first try of
  C1 in `2026-11-17-ssh-relay-design.md` §15): what the relay design took for "1Password does not answer a relay without
  a parent" was the same hidden prompt.

1Password's documentation of the SSH agent ("Suppressing background requests",
`https://www.1password.dev/ssh/agent/authorization`) says why: 1Password suppresses a prompt when the app or window the
request originated from is not in the foreground, puts a dot on its icon, and brings the prompt to the foreground when
the person selects the icon and then "SSH request waiting". So for a relay started from a task, the application is
op-reason, and op-reason's window, which never takes the focus (on purpose, the language toggle design), is never in
front: every first request after 1Password locks, and after the relay starts again, needs a trip to the tray, which is
easy to miss before the request closes.

The user chose to have op-reason's window come to the front when they click Allow, so that 1Password's prompt comes at
once (§4).

## 2. Goal and non-goals

Goal:

- After a click on **Allow this call** or **Allow this terminal**, op-reason's window comes to the front before the
  request goes to 1Password, and stays there, without its buttons, until 1Password has answered: 1Password's prompt
  comes at once, without the tray.
- When the request is through, the window that had the front before the click gets it back, unless another window took
  it meanwhile.
- The window still opens without taking the focus. Nothing changes for **Deny**.
- Where 1Password does keep a prompt behind its icon, op-reason says where: in its window while 1Password asks, and in
  the notices that were written for the parent (§6).

Non-goals:

- The windows that come without a click: show-only mode, and the window without buttons of a request that an earlier
  answer let through, when 1Password is slow (to unlock, for example). Nothing gives the relay the right to the front
  then, and taking it uninvited would take keys from the terminal. Their prompts may still wait behind the icon; the
  window's last line says where.
- The windows of `op` calls (`op-reason.exe`'s and the launcher's): 1Password's prompt for `op` comes as before.
- The relay's options and records, the dotfiles' setup (the scheduled task), a version.

Anti-goals (a failure even if it works):

- op-reason's window takes the focus before the person answered: keys typed in the terminal would go to it.
- The window takes the front back from a window that the person or 1Password brought to the front meanwhile.
- A relay started in a terminal program no longer gets its prompt at once (it did before).

## 3. What 1Password and Windows do

### 3.1 1Password

- **What it documents**: the suppression above, the dot, and "SSH request waiting". It does not say how it finds the
  application of a request, whether "in the foreground" means a window or a process, or whether a suppressed prompt
  comes by itself when the application comes to the front later. Its community forum adds that the suppression came in
  8.10.4 (2023) and caught another relay too (`winssh-pageant`, a bridge started at logon, whose requests 1Password
  took for background requests until it fixed that "false positive" for that program).
- **What the checks showed** (§1): the application it names is the terminal program for a relay started in one (it
  walks up from the relay to an application it knows; `2026-11-17-ssh-relay-design.md` §15), and op-reason for a relay
  whose parent is the Task Scheduler's host. A request it does not show waits about 60 seconds, then it closes the
  request without an answer (the relay's `no-answer`).
- **Not known before the checks of §11**: whether a window of op-reason in front is enough (op-reason's window is a
  tool window, `WS_EX_TOOLWINDOW`), and whether 1Password compares the process of the window in front with the
  application exactly or through its process tree. The design does not depend on the first beyond the window
  belonging to the process that 1Password names, and does not depend on the second (§5.1).
- **Answered by the check of §13**: op-reason's tool window in front is enough (a task's relay: 1Password's prompt
  came half a second after the window came forward, naming op-reason), and so is a window of a process under the
  application that 1Password names (a relay started in a terminal program, with op-reason's window in front: the
  prompt came at once, naming the terminal program). 1Password looks at the window in front through the process tree,
  at least from the application down.

### 3.2 Windows

- **`SetForegroundWindow`** (Microsoft's documentation, 2025-10): a process may set the foreground window only if it is
  a desktop application, the foreground process has not locked it (`LockSetForegroundWindow`) and no menu is active,
  and at least one of: the foreground lock time-out has expired (`SPI_GETFOREGROUNDLOCKTIMEOUT`), it is the foreground
  process, it was started by the foreground process, there is no foreground window, **it received the last input
  event**, or a debugger is involved. "It is possible for a process to be denied the right to set the foreground window
  even if it meets these conditions", and "an application cannot force a window to the foreground while the user is
  working with another window".
- Right after the click on its button, the relay received the last input event: the click is input to its window, even
  though the window does not activate (`WM_MOUSEACTIVATE` answers `MA_NOACTIVATE`). op-reason already relies on this:
  a click on a reference's link hands the right on with `AllowSetForegroundWindow` so that the 1Password app comes to
  the front (`2026-11-17-refs-in-window-design.md` §5.4, checked with the real app in F3).
- **`WS_EX_NOACTIVATE`**: "A top-level window created with this style does not become the foreground window when the
  user clicks it. The system does not bring this window to the foreground when the user minimizes or closes the
  foreground window. … To activate the window, use the SetActiveWindow or SetForegroundWindow function." So the window
  keeps its style, and comes forward only when the relay asks; and when 1Password's prompt closes, Windows does not
  bring op-reason's window back by itself.
- **`AllowSetForegroundWindow`**: a process that can set the foreground window can let another process do it, until the
  next input that is not directed at that process.

## 4. Approaches

| Approach | What it means | Trade-off |
| --- | --- | --- |
| Approve from the tray | no change to op-reason; the README and the notices say where the prompt waits | a trip to the icon at each first request after 1Password locks and after the relay starts again; missed, the request fails after 60 seconds (checked: it works) |
| Start the relay as a child of the terminal program | 1Password names the terminal program, which is in front while the person works in it | the relay lives and dies with one terminal program, not with the logon; other programs in front still hide the prompt |
| **The window comes to the front after Allow (chosen)** | the click gives the relay the right; 1Password sees op-reason in front | the terminal loses the focus until 1Password answers (it gets it back); depends on 1Password counting op-reason's window (checked in §11) |

Within the chosen approach:

| Decision | Chosen | Not chosen, and why |
| --- | --- | --- |
| When | Right after the click on a button that allows, before the request goes to 1Password | When the window opens: it would take keys from the terminal, and Windows may refuse it without input; after the request has gone: 1Password may have decided already (the documentation does not say that it looks again) |
| How | `SetForegroundWindow` on the click's right; the window keeps `WS_EX_NOACTIVATE` | Dropping `WS_EX_NOACTIVATE`: every click (the language switch, a link) would activate it, against the language toggle design; `AttachThreadInput` or a simulated key to force the front: works around Windows' rules |
| The program that started the relay in front | The window stays behind | Come forward anyway: if 1Password compares the process in front with the application exactly, a relay started in a terminal program would lose the prompt it gets today (an anti-goal) |
| 1Password's prompt | The relay lets the process that serves 1Password's pipe set the foreground window, before the request goes | `ASFW_ANY` (any process); nothing (1Password brings its prompt forward today without help, but then the foreground process was not op-reason) |
| After 1Password answers | The window gives the front back to the window that had it, if nobody else took it | Leave it to Windows: it does not bring op-reason's window back (§3.2), and where the front lands after the prompt is not known |

## 5. The design

### 5.1 When the window comes to the front

- per-call mode, a click on **Allow this call** or **Allow this terminal**: right after the click, before the answer is
  remembered and the request goes to 1Password, the relay calls `SetForegroundWindow` on its window.
- Not when the window in front belongs to the program that started the relay: one of the relay's ancestors (read once
  at start: PID, creation time, image name; a later process with an ancestor's PID does not count), `explorer.exe`
  excepted. That is the terminal program of a relay started in one, which 1Password takes for the application and sees
  in front already. A relay started from a task has the Task Scheduler's host above it, which owns no window on the
  person's desktop; Explorer, which starts programs from the desktop and the taskbar, is not an application for
  1Password (it named the terminal program above it, not Explorer).
- Never when the window opens, for **Deny**, in show-only mode, for the window without buttons of a slow answer, for
  `op` calls.
- If Windows refuses, nothing else is tried: the window stays where it is, as before, and 1Password may keep its prompt
  behind its icon (the window's last line says where, §6).

### 5.2 While 1Password answers

- The window stays in front, without its buttons ("SSH key: 1Password is asking"), as before after Allow.
- Before the request goes, the relay lets the process that serves 1Password's pipe (`GetNamedPipeServerProcessId` on
  its connection: the 1Password app) set the foreground window (`AllowSetForegroundWindow` with that PID), so that
  1Password's prompt can come in front of op-reason's window.
- Keys typed while the window is in front go to it and do nothing: it has no default button, and its buttons are gone.

### 5.3 Giving the front back

- When the window closes (the request is through or failed), it gives the front back to the window that had it before
  the click: only while op-reason's window still has the front or no window has it, and only to a window that still
  exists, is shown and is not minimized.
- A window that took the front meanwhile keeps it: 1Password's prompt, the window of Windows Hello, a window the person
  went to.
- When 1Password's prompt closes, Windows does not give the front to op-reason's window (§3.2); where it lands then is
  part of the checks (§11).

## 6. Where a hidden prompt waits

- The window without buttons ("1Password is asking": after Allow, in show-only mode, and when 1Password is slow to
  answer) gets a last line: "If 1Password's prompt does not come: the 1Password icon at the right of the taskbar (under
  ^ when hidden) → SSH request waiting" (「1Password の画面が出ないとき: タスクバーの右の 1Password のアイコン (隠れていれば ^
  の中) →「SSHリクエストを待機中」」). It is the way out wherever the window cannot come forward (§2), and when Windows
  refused it the front (§5.1).
- The notice "SSH key: 1Password did not answer" said that the relay's parent had ended and that 1Password does not
  answer such a relay, so start it from a scheduled task. A task's relay is in the same place (§1), so it now says that
  while the application that asks is not in front, 1Password keeps its prompt behind its icon (SSH request waiting) and
  closes it after about 60 seconds; with a parent that has ended, it adds that 1Password most likely takes the relay
  itself for that application.
- The notice "op-reason's SSH relay has no parent process" (or "lost its parent process") said that 1Password leaves
  the requests of such a relay unanswered. It now says that 1Password most likely takes the relay itself for the
  application, whose prompt comes while op-reason's window is in front (after Allow) and otherwise waits behind the
  icon.

## 7. Documents

- README.md and README.ja.md: the window paragraph of "Two modes" (it opens without the focus, comes to the front after
  Allow until 1Password answers, gives the front back, stays behind for its own program; Deny never takes it); the
  "Windows" item; "Starting it": a scheduled task that also repeats every minute (the check of §1 found that the task's
  "restart on failure" did not start a stopped relay again) and keeps its rule "Do not start a new instance" (§10), and
  why the window comes to the front, with the tray for the windows that come without a click.
- `docs/architecture/README.md`: the relay's threads (its ancestors at start, the parent), step 3 of a request (coming
  to the front, `AllowSetForegroundWindow`, giving back), step 5 (the notice), the tests.
- `docs/manual-verification.md` and `.ja.md`: checks S25–S31, and S15's expectation.
- `CHANGELOG.md`: the Unreleased section.
- Unchanged: the README animation and the HTML page of the relay's scene (they show the window at a click, without
  1Password's prompt, and copy none of the changed texts), the skill (agents do nothing differently).

## 8. Tests

- Linux (`cargo test` in `crates/op-reason-win`): `relay_front` (the program that started the relay in front: its
  parent, its terminal program, a creation time that cannot be read; not another program, no window in front, a reused
  PID, Explorer, a task's relay, a relay without a parent); `relay_ui` (the last line while 1Password asks, in both
  languages and all three cases; none in the window that asks; the notices say where the prompt waits and no longer
  send the person to a task).
- Windows CI (`tests/relay.rs`, real clicks), with a plain window of another process in front (`fake-ssh front`):
  op-reason's window opens behind it; Deny leaves the front where it was; after Allow op-reason's window is in front
  while the fake agent answers (2.5 seconds), then the front goes back to the plain window. With the plain window's
  process as the relay's parent (it starts the relay): after Allow op-reason's window never comes to the front.
- What CI cannot show: 1Password's prompt (there is no 1Password), and a person's own click (CI clicks with injected
  input, which Windows counts as input too). §11.

## 9. Left to the implementer

- The texts of the last line and of the notices; how the window finds its ancestors and the window in front; the
  tests' helpers.

## 10. Implementer's decisions

| Decision | Why |
| --- | --- |
| The relay's ancestors are read once at start (`peer::chain` of the relay: PID, creation time, name); the window in front counts as the relay's program when its process is one of them (same creation time when both can be read; by PID when one cannot), `explorer.exe` excepted | ancestors never change; the creation time keeps a reused PID out; a task's host (SYSTEM) cannot be opened and owns no window anyway |
| The check runs between the click and `SetForegroundWindow`; the window comes forward before the answer is remembered and the window loses its buttons | the right holds until the next input elsewhere: the shorter the gap, the safer |
| `AllowSetForegroundWindow` for the PID of the server of the relay's connection to 1Password's pipe, not `ASFW_ANY` | only 1Password needs it; a failure to read it costs nothing (1Password brings its prompt forward today without it) |
| The front is given back in the window's `Drop`, before it is destroyed | every way a window that came forward ends (answered, failed, stalled, closed by the person) goes through it; while it still has the front, the relay is the foreground process and may set it |
| No option to turn it off, no record of it | the user asked for the behavior and will check it; the checks watch the window in front (the manual checks note it) |
| A refused `SetForegroundWindow` is not tried again | Windows' rules decide (§3.2); the last line of the window says where the prompt waits |
| The tray's item is named as 1Password names it: "SSH request waiting" in English, 「SSHリクエストを待機中」 in Japanese (seen in the check of §1, without its trailing dots) | so that the person finds the words in the menu |
| `fake-ssh front` makes the plain window of the Windows tests (and, with a program, starts it as its child) | the window in front must belong to another process than the test's, which is the relay's parent; with a program, it stands for the terminal program the relay was started in |
| The toast test waits 300 ms before it reads the window in front | its first call now takes the front and gives it back |
| No plan document | the change is small, and §5 and §8 are its steps |
| After the first check (§12): the relay's windows draw into their device context at each `WM_PAINT` through a DC render target on Direct2D's software renderer (`CreateDCRenderTarget`, `D2D1_RENDER_TARGET_TYPE_SOFTWARE`, `BindDC`; Direct2D's default HWND target only if that cannot be made); the windows of `op` calls keep Direct2D's default | the relay's window stopped answering at its first frame, with Direct3D and DXGI loaded, where the GPU and the swap chain are the parts that the runner never had; a DC render target has no swap chain and does not wait for the display's refresh (the first build of this fix, an HWND target on the software renderer presenting at once, kept a swap chain); a window this small draws as fast without it; `op` calls show their windows every day without that, and are left as they are |
| After the first check: a watchdog thread of the relay records `window-stuck` (with the step: `factories`, `layout`, `create`, `show`, `target`, `draw`, `holder`, `toast`) when a window's thread has waited five seconds in one step, and `window-unstuck` (after how long) when it goes on; the steps are process-wide atomics of `window.rs` (when a step ends, the step around it is the one that runs again); the records' `ssh-relay` gets an optional `detail` | the first check could only say that the thread waited; the next one says where, without a debugger; the watchdog is started before any window, so that it runs whatever the window's thread waits for |
| After the second check (§13): the README names the task's rule for when it runs already, **Do not start a new instance** (the default) | with "Stop the existing instance", the trigger that repeats every minute would stop the relay and start it again every minute; with "Run a new instance in parallel", each start while the relay runs exits with 1 (one relay per logon session), which the task shows as its last result. The `0xFFFFFFFF` that the check reported as the task's last result is most likely the exit code of the relay that the check had killed (`Stop-Process` ends a process with -1), shown three seconds after the kill, before the trigger started the next relay; with the default rule no second relay starts |
| After the second check: the notifications keep Windows PowerShell's AppUserModelID, and name "Windows PowerShell" as their sender | a name of op-reason's own touches every notification of op-reason (`op` calls and the launcher too) and needs a registration, which `2026-11-15-windows-exe-design.md` §7.4 keeps opt-in: a change of its own |

## 11. Checks with the real app

With the builds of this pull request (`docs/manual-verification.md`, S25–S31), run on 2026-11-22 by the person who
asked for this change, with a script that did the steps and watched the windows:

- **The first build (head `f295109`, 04:10 and 04:43)**: the first window of a relay started from a scheduled task
  stopped answering before it was drawn, so no click was possible (§12).
- The next builds draw the relay's windows without the GPU and record a window that does not appear (§10, §12).
  **An HWND target on the software renderer (head `3f8f0a0`, 05:06)**: the window waited for a click until its 60
  seconds ran out; no click came (§13).
- **A DC render target, without a swap chain (head `14c0724`, 06:45–07:18)**: S25, S26, S28, S29 and S30 as
  expected. After Allow, 1Password's prompt came at once, without the tray, for a relay started from a task and for
  a new one (§13). S27 and S31 were not run. `docs/manual-verification.md` has the results too ("Results of
  2026-11-22").

## 12. The first check: a window that never drew itself

What happened (2026-11-22, 04:10–04:16, on the author's Windows; the relay started from a scheduled task, its parent
the Task Scheduler's host in session 0; 1Password locked and unlocked just before):

- op-reason's window "SSH key: allow this call?" appeared at the top right. Nobody saw it appear (the person was away
  then); when it was seen (a screenshot at 04:13:35, and the checks until 04:15) it was white inside, without any text,
  and its caption said "(Not Responding)". Nobody pressed a button. The window in front stayed the terminal program all
  along: the window never came to the front, as no click reached it.
- The relay's record of the signature came 321.8 seconds after the request (`gone`, no answer): its window's thread was
  waiting from 04:10:18 to 04:15:40, and then found the client gone (the check had stopped ssh after 150 seconds).
- Between, the relay's threads were all waiting (0.34 s of CPU in all); its window was the only top-level window besides
  the thread's IME window, and Windows called it hung (`IsHungAppWindow`).
- 1Password's log had one line about SSH: an authorization prompt that timed out at 04:11:18, 60 seconds after the
  relay got the request. No line said that a prompt waited behind the tray icon (as in the check before this change),
  and nobody saw a prompt of 1Password.
- Run again at 04:43 with the same build, watched every quarter of a second by the person and the check: the window
  appeared at 04:43:25.5 as "SSH key: allow this call?", white from the start, and Windows called it hung at
  04:43:30.7: it never answered once. 1Password's line came at 04:44:25.0, 60 seconds after the window appeared;
  nobody had clicked. The relay had loaded Direct2D, DirectWrite, Direct3D 11 and DXGI; its threads all waited, and it
  had recorded only its start.

The three questions, and what is known:

1. **Where the window's thread waited.** Not in the code of this change: nothing of it runs before a click (§5), and
   the click never came. The thread was in the opening of the window, before the loop that waits for the click: in
   that loop it would have ended at the 60-second answer wait (`timeout`), while it ended as `gone`, which it finds
   first only when it comes out of the opening after the client has gone. The opening (make the window, show it, draw
   it with Direct2D, write `holder.json`, start the notification) is the relay's since 0.4.0, and it has drawn every
   window of the checks before this one (S1–S24, and the check of the scheduled task a day before, with 0.5.0). Which
   part of it waited is not known for sure: the second run saw the window hung from its very first moment, never drawn,
   with Direct3D and DXGI loaded, which points at the first frame; but CI never stopped there (its runner draws without
   a GPU driver, and has no 1Password), and the thread's stack was not taken. The likeliest part is the first frame on
   the GPU (the first Direct2D drawing of a build that this machine had never run, presented through a swap chain at the
   display's refresh), so the relay's windows now draw into their device context on the software renderer (§10). If a
   window stops again, the relay records the step (`window-stuck`), and the check takes the thread's states
   (`-Step stuck` of the check's script).
2. **Whether a request reached 1Password before Allow.** No. In per-call mode the relay sends a signature on only after
   a click that allows, or for a call that an earlier answer allowed; this relay had just started and remembered no
   answer, and its record of the request has no answer. CI checks it: a signature left unanswered in the window never
   reaches the fake agent (`a_signature_waits_for_the_window_and_an_unanswered_one_is_not_remembered`). The relay had
   passed on the key list and `session-bind@openssh.com` (as always, before the window); whether one of those, or
   another client, made 1Password's prompt that timed out is not known (§13 adds what the later runs showed).
3. **Why the record came only with the next connection.** A record is written when the request ends; its thread waited
   in the opening of the window until 04:15:40. The check's `-Step status` connected to the relay's pipe at that
   moment (with `--status`), but the opening shares nothing with the threads that accept connections, so it is not
   known whether that connection (or something else then) ended the wait.

What changed (the next build):

- The relay's windows draw into their device context with Direct2D's software renderer (a DC render target), without
  the GPU, without a swap chain and without waiting for the display's refresh (§10). The windows of `op` calls are
  unchanged.
- A watchdog records a window that has not appeared within five seconds, with the step, and when it goes on (§10).
  `op-reason.exe --log` shows them ("op-reason's window stuck (draw)").

## 13. The second check: the prompt at once

The build that draws the relay's windows into their device context (head `14c0724`), 2026-11-22, 06:45–07:18, on the
author's Windows, with 1Password as in §1. The script registered a scheduled task with a logon trigger and a trigger
that repeats every minute (as the README says) on a pipe of its own, started the relay of S29 and S30 in the terminal
program, and noted the window in front and op-reason's window every 0.2 seconds. 1Password was locked and unlocked
before S25, S28, S29 and the second try of S30.

| Check | What happened |
| --- | --- |
| S25: a task's relay | op-reason's window appeared 1.5 seconds after ssh started, drawn, and the front stayed with the terminal program. At the click on Allow (25 seconds after ssh started) op-reason's window was in front at once, and half a second later 1Password's prompt, which named op-reason ("Allow op-reason to use SSH key"); nobody opened the tray. After the approval (with Windows Hello) the window closed, and the terminal program had the front again, as before the click |
| S26: again | No prompt and no dot: 1Password had approved this run of the relay. The window closed right after the click, too soon for the 0.2-second watch to see it in front; the front stayed with the terminal program |
| S28: a new relay | The script stopped the relay (`Stop-Process`), and the task's trigger that repeats every minute started a new one 48 seconds later. Its first request: in front at the click, 1Password's prompt 0.4 seconds later, naming op-reason; afterwards the front was the terminal program's again |
| S29: a relay started in the terminal program | The relay's ancestors were processes of the terminal program (four), then Explorer. With the terminal program in front, the window stayed behind at the click (§5.1), and 1Password's prompt came 0.6 seconds later, naming the terminal program. The front stayed with the terminal program |
| S30: the taskbar in front | First try: 1Password had not been locked since S29 and signed without a prompt; the window closed at once. Second try, with a new relay started in the terminal program and 1Password locked and unlocked: the person clicked the taskbar, then Allow. The window came to the front at the click, and 1Password's prompt came 0.4 seconds later, naming the terminal program. At the end the front went to the window that had it before the taskbar was clicked (another terminal program's): 1Password's prompt had the front when op-reason's window closed, so the window gave nothing back (§5.3), and Windows chose |
| S27, S31 | Not run |

Before the click, in S26 and S28, the person opened the menu of the hidden icons at the right of the taskbar: the
1Password icon had no dot, and no prompt had come (in S25 nobody looked). 1Password's log had no line about an
authorization prompt in any of these runs.

Every window of this check (six) appeared within 1.5 seconds, drew itself and answered until it closed; the relay
recorded no `window-stuck`. The last line of the window "1Password is asking" could be read (§6). The Windows
notification that comes with the window names "Windows PowerShell" as its sender, as since 0.1.0: op-reason borrows
that AppUserModelID and registers nothing (`2026-11-15-windows-exe-design.md` §7.3); a name of op-reason's own would
be a change of its own. No notice that 1Password did not answer came.

What it answers:

- **The change works as meant.** For a relay started from a task, 1Password's prompt comes at once after Allow,
  without the tray, also for a new run of the relay; a relay started in a terminal program gets its prompt at once, as
  before.
- **§3.1's open questions.** 1Password counts op-reason's tool window in front (S25, S28), and a window of a process
  under the application that it names (S30). For §5.1 this means that the window could come forward with the relay's
  own program in front too, and 1Password would still ask at once; the rule keeps the focus in that program, where
  coming forward helps nothing.
- **Where the front lands.** With the window that had it before the click (S25, S28: whether the window gave it back
  or Windows did, the 0.2-second watch cannot tell), or, when 1Password's prompt is in front at the end, where Windows
  puts it (S30).

Still not known:

- **The prompt that timed out in 1Password's log.** In each of the three runs where nobody clicked (§12, and the run at
  05:06 below), 1Password's log had the line of an authorization prompt that timed out, 60 seconds after the relay got
  the request. In this check every click came within 40 seconds of the request; there was no such line, and no prompt
  or dot before the click. Whether 1Password starts a 60-second wait with the key list or `session-bind@openssh.com`,
  which a click later than that would meet, is not known; the window waits 60 seconds for a click by default
  (`--answer-wait`).
- **Why the first build's window hung** (§12). None of the windows drawn without the GPU hung (six here, one at
  05:06), but the cause is not proven.

The run before it (05:06, head `3f8f0a0`, an HWND target on the software renderer): the relay's record says that its
window waited for a click until its 60 seconds ran out (`timeout`), with no `window-stuck`; no click came, and ssh
failed after 63 seconds. The script never found the window: it passed `$null` for the title to `FindWindow`, which
PowerShell turns into an empty title (fixed for the run above). 1Password's line of the prompt that timed out came at
the same moment as the relay's time-out.
