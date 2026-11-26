# op-reason: after Allow, show where 1Password's prompt is — design

- Date: 2026-11-26
- Status: approved (the user asked for a clearer screen after Allow: approve in 1Password's prompt, or open it from the
  1Password icon in the tray, with a picture of what is asked; of three ways, the user chose A: watch whether
  1Password's prompt comes to the front, and point at the tray when it does not. This document records the design and
  the implementer's decisions)
- Builds on: `2026-11-22-front-after-allow-design.md` (the window comes to the front after Allow,
  `AllowSetForegroundWindow` for the process that serves 1Password's pipe, where a hidden prompt waits) and
  `2026-11-26-modern-window-design.md` (the look: the band and its picture, the palettes)

## 1. What and why

What happened (2026-11-26, per-call mode, a relay of op-reason 0.7.0 started from its scheduled task):

- 1Password had just been started again and was locked. A request came; the person clicked **Allow this call** at
  08:41:31.
- 1Password did not bring its prompt to the front. Its log said, at 08:41:32, that it notified the user through its
  tray icon that a background prompt was waiting; it showed only a dot on its icon at the right of the taskbar and "SSH
  request waiting" in its menu.
- The person did not notice the tray, and unlocked 1Password by other means 23 seconds later; the signature passed 29.5
  seconds after the click.
- Two other requests timed out in the days before, at the same moments as 1Password's "ssh authorization prompt timed
  out".

The checks of the window in front after Allow (`2026-11-22-front-after-allow-design.md` §11–§13) all unlocked 1Password
before the request: a request while 1Password is locked was never checked. Why 1Password kept the prompt behind its icon
here is not known; whether op-reason's window was in front is not known either (the relay records neither).

The user asked for a clearer screen after Allow: either approve in 1Password's prompt, or open it from the 1Password
icon in the tray ("SSH request waiting"), with a picture of what is asked. Of three ways (§4) the user chose A: watch
whether 1Password's prompt comes to the front after Allow; when it does, say so briefly with a picture of its question;
when it does not, light the window up and point at the tray.

## 2. Goal, non-goals, anti-goals

Goal:

- After a click on **Allow this call** or **Allow this terminal for N minutes** (per-call mode), the relay watches
  whether 1Password's prompt comes: a window of 1Password comes to the front, or a new window of 1Password appears (§5).
- When it comes: the window says "Approve in 1Password's window", small, with a picture of the question 1Password asks.
- When it has not come within 2 seconds: the window lights up (a border in the caution color), and a small window next
  to the notification area points at the tray with an arrow: "the 1Password icon (under ^ when hidden) → SSH request
  waiting", and the seconds left until 1Password closes the request (about 60).
- A wrong guess costs nothing more than a guide: a prompt that comes later than 2 seconds takes the guide away again;
  a window of 1Password taken for the prompt leaves the band's line about the tray.
- The relay records what it saw, for the next time a prompt does not come (§7).
- The checks with the real app cover a request while 1Password is locked.

Non-goals:

- The windows that come without a click (show-only, a request that 1Password is slow to answer): nothing gives the
  relay a moment to watch from; their band names the tray already.
- `op` calls: their window never comes to the front and asks nothing.
- Why 1Password keeps a prompt behind its icon: 1Password decides it; the checks note what they see.
- Opening the tray or 1Password's menu for the person (input that the person did not make).

Anti-goals (a failure even if it works):

- The small window takes the focus or clicks, or covers 1Password's icon or menu.
- A guide that stays after the request has ended.
- Anything that changes when the window opens, how it is answered, or who has the front.

## 3. What op-reason can see

- The process that serves 1Password's pipe (`GetNamedPipeServerProcessId` on the relay's connection; the relay hands it
  the right to the foreground already, `2026-11-22-front-after-allow-design.md` §5.2).
- The window in front (`GetForegroundWindow`) and its process; every visible top-level window and its process
  (`EnumWindows`); a process's image name.
- Not what a window of 1Password says (that would need UI Automation and 1Password's own words, which change), nor
  1Password's log (another application's files).
- 1Password asks Windows Hello to unlock or to approve: the window in front is then Windows' (`CredentialUIBroker.exe`),
  not 1Password's.

## 4. Approaches

| Approach | Trade-off |
| --- | --- |
| **A: watch, and point at the tray when the prompt does not come (chosen)** | small when the prompt comes; points where the person missed it; depends on telling 1Password's prompt from other windows (§5), where a wrong guess only adds or keeps a guide |
| B: always show a big guide after Allow, with both ways side by side | no guessing, but big every time, and the eye does not go to the bottom right |
| C: find out first why 1Password hides the prompt, and fix that | depends on 1Password; may not be fixable, and the guide would wait for it |

Within A:

| Decision | Chosen | Not chosen, and why |
| --- | --- | --- |
| What counts as the prompt | after the request went: the window in front changes to a window of 1Password (the pipe's server, or a process named `1Password.exe`) or of Windows Hello (`CredentialUIBroker.exe`); or a top-level window of 1Password appears that was not there; only windows a person can see count (§5) | The window in front only: a prompt shown behind would not count (a guide too many); any window of 1Password visible: its main window may be open all day |
| How long to wait | 2 seconds after the request went | The checks of 2026-11-22 saw the prompt 0.4–0.6 seconds after the click; 5 seconds: the person may already look away |
| The light | a border of 2 DIPs in the caution color inside the window, pulsing once a second (steady with Windows' animation effects off), and the frame's border in that color (Windows 11) | Flashing the caption or the taskbar: the window is a tool window, out of the taskbar |
| The pointer | a layered window like the toasts (click-through, never active, topmost), above the notification area of the primary taskbar, its arrow at the notification area | A balloon of op-reason's own icon: it points at op-reason's icon, not 1Password's; a Windows notification: it goes to the notification center and says nothing about where |
| Where the notification area is | the window `TrayNotifyWnd` in `Shell_TrayWnd`; else the end of the taskbar (the window rectangle of `Shell_TrayWnd`); else the bottom right of the primary screen's work area | `SHAppBarMessage(ABM_GETTASKBARPOS)`: it asks Explorer, and a busy Explorer would hold up the relay's thread |
| The seconds left | about 60 from when the request went to 1Password, counted down | 1Password does not say when it started its wait; the checks saw 60 seconds |

## 5. The watch

- It starts when the request goes to 1Password after a click that allows (after the window came to the front and
  1Password got the right to the foreground). The relay notes the window in front then, and the windows of 1Password's
  processes that a person can see.
- Every 100 ms while 1Password has the request (the loop that waits for the answer), it looks again:
  - **seen** when the window in front is another one than at the start and belongs to a process of 1Password or to
    `CredentialUIBroker.exe`, or when a top-level window of 1Password is there that was not at the start;
  - **hidden** when 2 seconds have passed without that.
- Only windows that a person can see count, the one in front too: visible, not minimized, not hidden by DWM (cloaked:
  another virtual desktop, a suspended app), and not a menu or a tooltip (1Password's menu in the tray is a window of
  its process; opening it is not the prompt).
- **seen** holds; **hidden** turns into **seen** when the prompt comes later.
- The processes of 1Password: the pipe's server, and processes whose image name is `1Password.exe`.

## 6. What the person sees

| State | The window | The pointer |
| --- | --- | --- |
| the first 2 seconds | "SSH key: 1Password is asking", the band as before (allowed here; the tray line) | — |
| seen | the band: "Approve in 1Password's window" / "It asks whether the SSH key **GitHub** may be used. If it went behind other windows: the **tray at the bottom right**", with a sketch of 1Password's question (a dialog with a key, two lines of words, two buttons) instead of the screen | — |
| hidden | lit up: a border of 2.5 DIPs in the caution color inside the window, pulsing once a second (steady with Windows' animation effects off), and the frame's border in that color (Windows 11); the band in the alert colors: "1Password's prompt did not come to the front" / "Open it from the **1Password icon at the right of the taskbar** (under ^ when hidden) → SSH request waiting", with the screen, its tray ringed and an arrow at it; under the band's place, a bar and "1Password closes the request in about N s" (the window's countdown, 60 seconds from the request) | shown: a card with a caution border and an arrow at the notification area: "1Password's prompt is waiting in the tray", "1Password icon (under ^ when hidden) → SSH request waiting", "closes in about N s", and a picture: 1Password's menu with its item "SSH request waiting" highlighted, a line to its icon (with a dot) in the tray, `^` |

The window keeps its title ("SSH key: 1Password is asking") and its footer ("Deny any 1Password prompt that comes
without this window"). The pictures are sketches drawn with Direct2D's shapes and op-reason's own words, not copies of
1Password's windows.

Texts (Japanese):

| Key | English | Japanese |
| --- | --- | --- |
| seen | Approve in 1Password's window / It asks whether the SSH key {0} may be used. If it went behind other windows: the tray at the bottom right | 1Password の画面で承認してください / SSH の鍵 {0} を使ってよいかを聞いています。ほかの窓の後ろに隠れたら右下のトレイから |
| hidden | 1Password's prompt did not come to the front / Open it from the 1Password icon at the right of the taskbar (under ^ when hidden) → SSH request waiting | 1Password の画面が前に出ていません / タスクバーの右の 1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」から開いてください |
| closes | 1Password closes the request in about {0} s | 1Password が依頼を閉じるまで 約 {0} 秒 |
| pointer | 1Password's prompt is waiting in the tray / 1Password icon (under ^ when hidden) → SSH request waiting / closes in about {0} s / SSH request waiting (the menu's item) | 1Password の画面はトレイで待っています / 1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」 / 約 {0} 秒で閉じます / SSHリクエストを待機中 |

`{0}` of "seen" is the key's comment in 1Password (else its fingerprint).

### 6.1 Where the pointer goes

- The taskbar's window rectangle (`Shell_TrayWnd`; a rectangle is read without a message to Explorer) and its screen
  (the screen that holds it, with its DPI); its edge from the two: a bar wider than tall at the top or the bottom, one
  taller than wide at the left or the right, on the side of the screen's middle where the bar's middle is
  (`relay_prompt::taskbar_side`; a taskbar that hides itself lies mostly outside the screen, on its edge). The
  notification area `TrayNotifyWnd` in `Shell_TrayWnd` when it lies on that taskbar, else the 160 DIPs at the taskbar's
  end; without a taskbar, the bottom right of the primary screen's work area.
- The card goes next to the taskbar on the side of the screen (above a taskbar at the bottom, below one at the top, to
  the side of one at the left or the right), 2 DIPs from it, inside the screen; its arrow's tip at the middle of the
  notification area, at least the card's corner away from the card's corners (`relay_prompt::spot`).

## 7. The record

The `ssh-sign` record of a request after a click that allows gets `prompt`: `front` (op-reason's window came to the
front at the click), `seen` (seconds from the request to 1Password's prompt, to a tenth; null when it never came) and
`hint` (seconds from the request to the guide of §6's hidden state: the window lit up, and the pointer unless it could
not be made; null when the guide never came; kept when the prompt came later), for example
`"prompt":{"front":true,"seen":null,"hint":2.0}`. Other records have no `prompt`. `op-reason --log` does not show it. So
the next time a prompt does not come, the record says whether op-reason's window was in front.

## 8. Tests

- Linux: the watch (§5) as a function of what is seen and when; the texts of the three states and the pointer in both
  languages; where the pointer goes for a taskbar at each edge; the record's `prompt`.
- Windows CI: after Allow with the fake agent answering after 4 seconds and no window, the pointer appears and goes with
  the window, and the record has `seen: null` and a `hint`; with the fake agent showing a window that takes the front
  (it stands for 1Password's prompt), no pointer, and `seen` under 2 seconds.
- Screenshots: the window and the pointer when the prompt does not come, and the window when it comes.

## 9. Documents

README (what the window shows after Allow, the pointer), architecture (the watch, the pointer, the record),
manual checks (a request while 1Password is locked; the pointer with the taskbar elsewhere), CHANGELOG.

## 10. Implementer's decisions

| Decision | Why |
| --- | --- |
| The watch starts when the request goes (after the window came to the front and 1Password got the right to the foreground), and looks every 100 ms from the loop that waits for 1Password's answer | the loop pumps the window's messages every 10 ms already; a look reads the window in front and enumerates the top-level windows, cheap at that rate |
| "1Password" is the pipe's server and any process named `1Password.exe`; Windows Hello's `CredentialUIBroker.exe` counts only as the window in front | 1Password's windows may belong to another process of the app than the one that serves the pipe; Windows Hello asks for 1Password when it is locked or asks to approve; a new window of Windows Hello's broker could be any other app's |
| Only windows a person can see count (§5), the one in front too | DWM hides cloaked windows (another virtual desktop, a suspended app); a hidden or minimized window of 1Password in front shows nothing (a code review found that it would have counted); 1Password's menu in the tray is a window of its process, and opening it is following the guide, not the prompt |
| The hidden state uses the window's countdown (a bar and the seconds) for the 60 seconds | one countdown in the window, as for the answer wait; the bar is in the caution color already |
| The pointer is a window of the relay's window thread, dropped with the watch | it lives exactly as long as the request; no thread of its own |
| The pointer is not shown again after the prompt was seen | seen holds (§5); the band of the seen state still names the tray |
| `hint` stays in the record when the prompt came later | the record says that the guide was needed, and how long the prompt took |
| The CI stand-in for 1Password's prompt is a window of the fake agent (`<dir>/prompt-window`), which the relay lets set the foreground window as it lets 1Password | the same path as the real prompt: the pipe's server, the right to the foreground, a window in front |
| No pointer after op-reason's window was closed with its close button while 1Password asks | the person put the window away; a pointer alone would point at a request whose window is gone |
| The guide stays until 1Password answers, also when the ssh client has gone (Ctrl+C) | the request is still with 1Password, and the window stayed until its answer before this change too; the relay cannot take a request back from 1Password |
| With a taskbar at the top, the pointer may cover the top of op-reason's window (both at the top right of the screen) | the pointer lets clicks through, so the close button still works; Windows 11 keeps its taskbar at the bottom |
| The lit border pulses with ten frames a second | each frame draws the whole window with the software renderer; ten frames keep a pulse a second smooth at half the work of twenty |
| The pointer's card is 372 DIPs wide | at 340 the Japanese title left its last character alone on a second line (CI's screenshot) |
| No plan document | §5–§8 are its steps, and it builds on the parts of `2026-11-26-modern-window-design.md` |
