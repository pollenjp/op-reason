# op-reason: toasts for signatures that pass without a window, the screen of the windows, fewer notifications — design

- Date: 2026-11-18
- Status: approved (the user chose the top left and one card per call on a page that played three motions side by
  side, and approved the design of the toast, the screen of the windows (A) and the count of notifications (B) as a
  whole; this document records that design and the implementer's decisions). §11 records the two changes the user
  asked for after the first checks with the real app (2026-11-19).
- Builds on: `2026-11-17-ssh-relay-design.md` (§5.4 remembering, §5.5 the windows one at a time, §6 the window) and
  `2026-11-15-windows-exe-design.md` (the window and the toast of `op-reason.exe`).

## 1. What and why

Three changes to what op-reason shows on Windows, found in the checks of 0.4.0 with the real 1Password app
(`docs/manual-verification.md`, S1–S18):

- **The toast.** A signature that passes without a window (a later request of an allowed call, a command typed in a
  terminal that was allowed for 60 minutes) shows nothing today: who signed, and for what, is only in the records. A
  small card at the corner of the screen, which fades away by itself, shows it.
- **A: the screen of the windows.** op-reason's window always opens at the top right of the primary display's work
  area (`window.rs`: `MonitorFromPoint(0, 0)` and `SPI_GETWORKAREA`). In S2 it went unnoticed twice on the main display
  while the person worked on another, and the signature failed after 60 seconds. The window should open on the screen
  in use.
- **B: the count of notifications.** Every window comes with a Windows notification, which stays in the notification
  center after the window has closed (no tag, no group). In S11 the one notice that mattered ("1Password did not ask")
  was lost among them. A window's notification should go with its window.

## 2. Goal and non-goals

Goal:

- per-call mode: a signature that passes without a window gets a toast at the top left: the caller, the command and
  the destination, why it passed and the reason.
- The signatures of one call share one card; they never push the cards of other calls away.
- op-reason's windows and the toasts open on the screen of the foreground window.
- A window's Windows notification is removed when the window closes.
- The READMEs (English and Japanese) show where each of them appears, in an animation and in an HTML page.

Non-goals:

- show-only windows (one before every request) stay as they are. Whether a signature whose window closed at once
  should also get a toast is left until the toasts have been used.
- Nothing happens on a click on a toast (no log opens): a toast lets clicks through to the window below.
- No setting turns the toasts off: one call is one card, so they do not pile up.
- No toast for a request of a denied call, a failure or a time-out (the records keep them as now).
- Nothing changes on Linux (`op-reason`, `op-reason-ssh`), and the records keep their shape.
- The launcher's notification (the PowerShell entry and the fallback runner) stays as it is: B is about
  `op-reason.exe`'s notifications (`toast.rs`).

Anti-goals (a failure even if it works):

- A toast takes the focus, or takes clicks or keys (op-reason's windows answer only a press and a release of the mouse).
- A window that wants an answer and a toast that only tells are hard to tell apart.
- The toasts add entries to the notification center.

## 3. Approaches

| Decision | Chosen | Not chosen, and why |
| --- | --- | --- |
| Where the toasts go | The top left of the screen (the user's choice on the page that compared the motions) | The top right: where op-reason's window opens; windows that want an answer and notices that only tell would mix |
| One call's signatures | One card per call (×N and the last destination; the user changed to this after first choosing one card per signature) | One card per signature: ansible signs dozens of times in one run, and three cards would keep replacing each other |
| How the toast is made | A small window of op-reason's own (Direct2D, as the window) | A Windows notification (WinRT toast): it comes at the bottom right, stays in the notification center (the opposite of B) and cannot be placed |
| Pull requests | One for the toast, A, B and the documents | Three: the toast places itself as A does, and one round of checks with the real app covers all three |

## 4. The toast

### 4.1 When

- per-call mode, a request that an earlier answer covered (`ask::Outcome::Passed`: Allow this call, or Allow this
  terminal) and that 1Password signed (the record's result is `passed`).
- Not when 1Password took more than a second and the window without buttons came (`Watch::Late`): that request had a
  window. A late window that never came (its turn did not come before the answer) does not count as one.
- Never for a request that had a window, a request of a denied call (`Refused`), a failure, show-only mode, or a call
  of `op` (`op-reason.exe`'s own window).

### 4.2 Where

- The work area of the screen of the foreground window when the card comes (as for the windows, §5), at its top left:
  12 DIPs from its edges, 8 DIPs between cards.
- A new card goes at the top; the others move down.
- While cards are shown, a new card joins them on their screen (cards never move to another screen). The next card
  that comes when none is shown chooses the screen again.

### 4.3 What it says

Three lines, in the language of the windows (the one chosen in a window, then `OPREASON_LANG`):

1. `Passed · <caller>` / `通過 · <呼び手>`, then ` ×N` when N is 2 or more. The caller is the short name of the window's
   notification (`relay_ui::caller_short`).
2. `<command> → <destination>`: the client's command line as the window shows it (Command), and its destination.
   Without a destination, the command alone. The command is cut with an ellipsis so that the destination stays whole.
   A card of several signatures shows the last signature's.
3. `<why it passed> · <reason>`: "allowed for this call" / 「この呼び出しの許可」 (Allow this call) or "allowed for this
   terminal" / 「ターミナルの許可」 (Allow this terminal), then the reason, or "no reason given" / 「理由なし」.

The texts are in `relay_ui`'s table with the window's.

### 4.4 One card per call, and time

- The signatures of one call (the mark: root, key, reason) share one card. Each one adds one to ×N, puts its line 2 in
  and starts the four seconds again; the card glows once (the "bump").
- Other calls get a card each, three at most. A fourth card makes the oldest one (the card whose last signature is the
  oldest) go at once; `+N` under the third card counts the cards pushed out until their four seconds have passed (§11).
- A card is drawn at 70% opacity, so that the text behind it shows through (§11).
- A card stays four seconds after its last signature, then fades out in 0.8 seconds; the cards below close up.
- ×N counts the signatures that the call passed without a window since its window. When its card has gone and the
  call signs again, the count goes on (decided before this document).
- The motion: a card slides in and the others move in 240 ms, a card pushed out goes in 140 ms, the bump lasts 420 ms
  (the values of the page that compared the motions; to be tuned on a real screen).
- With Windows' "Animation effects" off (`SPI_GETCLIENTAREAANIMATION`), the cards neither slide nor fade: they appear
  and go at once (decided before this document).

### 4.5 Behavior

- It never takes the focus (`WS_EX_NOACTIVATE`), and lets clicks and the pointer through to the window below (a
  layered window with `WS_EX_TRANSPARENT`). Always on top. Not in the taskbar or Alt+Tab (`WS_EX_TOOLWINDOW`).
- It is not a Windows notification: nothing goes to the notification center, and it makes no sound.
- It looks unlike op-reason's window that wants an answer (top right): smaller, no caption, no buttons.

### 4.6 How it is made

- The relay process keeps one UI thread that owns the toasts' window; the signing threads tell it through a channel.
  (The windows that ask are made and served by the thread of their request, as now.)
- Drawn with Direct2D and DirectWrite, as the window. The manifest is per-monitor DPI aware (v2): the toasts follow the
  DPI of their screen.
- What decides the cards (one per call, the fourth pushing out the oldest, the time, the counts) is portable code
  without Windows, tested on Linux.

## 5. A: the screen of the windows

- op-reason's windows (the relay's and those of `op-reason.exe`'s calls) open at the top right of the work area
  (`GetMonitorInfoW`'s `rcWork`) of the screen of the foreground window when the window is made
  (`GetForegroundWindow` → `MonitorFromWindow`), with that screen's DPI (`GetDpiForMonitor`).
- No foreground window, or one that cannot be read: the primary screen, as now.
- The toasts choose their screen the same way (§4.2), at its top left.
- A window that is open stays where it opened when the foreground window moves to another screen (decided before this
  document).
- On which screen 1Password's prompt appears is checked with the real app after this pull request. If it is not the
  screen of the foreground window, the direction is discussed then (only the choice of the screen would change).

## 6. B: the count of notifications

- The Windows notification of each window (the relay's windows and `op-reason.exe`'s; `toast.rs`) gets a tag of its
  own and a group shared by all of them.
- When the window closes, its notification is removed from the screen and from the notification center: after Allow,
  after Deny, when the person closed the window, and when the request ended and the window closed (the window without
  buttons too).
- A per-call window that closed at its 60-second time-out leaves its notification (the sign of a request that went
  unanswered; decided before this document).
- The one-time notices stay as they are: 1Password did not ask, 1Password did not answer, the relay has no parent
  process or lost it.
- A notification left behind by a process that died is accepted.

## 7. Documents

- README.md and README.ja.md, in "SSH keys: the relay": a subsection on where the windows, the toasts and the
  notifications appear (op-reason's window at the top right of the screen of the foreground window, the toasts at its
  top left, Windows' notifications at the bottom right, gone when their window closes).
- An animation, made by `docs/images/make_readme_svgs.py` in English and Japanese (the SVGs are never edited by hand;
  GitHub shows them as images, so they move with CSS keyframes only): a toast comes, cards merge (×N), move, go, and
  the window, the toasts and the notifications keep to their corners.
- An HTML page in `docs/html/` that plays the motion scene by scene (pause, step), made from the page that compared the
  motions, with the values of the implementation (seconds, counts, texts). One file, no outside files; its links use
  `https` and open in a new tab. The subsection links it with a line `🌐 [HTML version: …](…)`.
- `docs/README.md` (the list and the map) points to the page.
- `docs/manual-verification.md` and `.ja.md`: checks of the toast (comes, merges, pushes out, fades), of A (the screen of
  the windows), of B (removed, left) and of the screen of 1Password's prompt.
- `docs/architecture/README.md`: the window's screen, the toasts' thread and the notifications' removal.
- `CHANGELOG.md`: the Unreleased section.

## 8. Tests

- Linux (`cargo test` in `crates/op-reason-win`): the toast's texts (both languages, both answers, no reason); the cards
  (one per call, ×N, line 2 of the last signature, the four seconds started again, a fading card that signs again comes
  back, the fourth pushing out the oldest, fading and closing up, the count going on after the card has gone and
  starting again for a new answer, no motion with animation effects off, when to draw next; after §11: the opacity, 70%
  when shown, from 70% while fading and at 70% under a bump's ring; `+N`, counted until each pushed-out card's four
  seconds have passed, the card whose last signature is the oldest pushed out (of two at once, the lower one), a
  pushed-out call that signs again coming back).
- Windows CI (`tests/relay.rs`, real clicks): after Allow, the same call signs three times: one toast window, visible,
  "×3" among its texts, layered, click-through (`WindowFromPoint` does not find it), never active, at the top left of
  the work area of the foreground window's screen; gone after about five seconds. Four other calls that pass one after
  the other: three cards and `+1`, the last line of the window's text.
- Windows CI (B): while a window is open its notification is in the notification center (`ToastNotificationHistory`
  read with the same AppUserModelID), and after the window has closed it is not; after a per-call time-out it stays.
  Where the runner keeps no notification history, the test says so and checks nothing.
- The CI screenshots add the toasts (a call of three signatures and another call, after three calls of which they push
  two out, so that `+2` shows) and the whole screen with them.
- `make_readme_svgs.py --check` fails when the texts or the values it copies from the Rust sources (the toast's texts,
  its sizes and times) differ, and when the HTML page no longer has them.

## 9. Left to the implementer

- The English texts; the toast's size, colors and type sizes (as the window's).
- The UI thread and the channel; how the layered window is drawn.
- Whether `ToastNotifier::Hide` or `ToastNotificationHistory::Remove` removes the notification.
- How the tests are split (as much as possible in the portable code).

## 10. Implementer's decisions

- **Line 2 is the client's command line**, the Command of the window, not the root's: the root of a Claude Code call is
  the shell of its tool call, whose command line wraps the command in a snapshot script and reads as noise in one line,
  while the client's command line names the host and, for git, the repository (`git-receive-pack 'me/r.git'`). The
  command is cut by DirectWrite at the width left after ` → <destination>`.
- **Texts**: line 1 `Passed` / `通過` in green, the caller in bold black, ×N in a green tag; line 3 in gray. The English
  answer words are "allowed for this call" and "allowed for this terminal".
- **The look**: the window's colors and font (light yellow `#FFFFE0`, black, Segoe UI 12 and 11 DIPs), a card 300 DIPs
  wide with rounded corners and a gray border, a green check at the left, and a thin line at the bottom that shortens
  over the four seconds. Every line has the same height (uniform line spacing), so a card is 70 DIPs high whatever font
  DirectWrite falls back to.
- **One layered window for all the cards** (per-pixel alpha, `UpdateLayeredWindow`), hidden when no card is shown. Its
  window text lists the cards' first lines (newest first), for tests and screen readers.
- **The count's key** is the call's mark and the record ID of the answer that covers it, so a call asked again after its
  answer ran out starts from one. Counts not used for the relay's time limit are forgotten.
- **The UI thread starts with the first toast** (show-only never starts it); the signing threads wake it with an event.
  A toast window that cannot be made costs nothing else: the signature is answered and recorded as before.
- **B**: tag `<process id>-<n>`, group `op-reason`. The notification is shown on its worker thread, which then waits;
  removing it is a message to that thread, so a window that closes before its notification has been shown still
  removes it. The worker calls both `ToastNotifier::Hide` (the notification on the screen) and
  `ToastNotificationHistory::RemoveGroupedTagWithId` with the same AppUserModelID (the notification center).
  `op-reason.exe` removes its notification when its window closes (the record's `toast` field is unchanged), and leaves
  it when no window could be shown (then the notification was the only sign of the call).
- **Documents**: the animation is `readme-6-relay-screens.{en,ja}.svg`; the HTML page is `docs/html/relay-screens.html`,
  one file with both languages (English, or Japanese with `#ja`).
- **Checks with the real app**: S19–S24 in `docs/manual-verification.md`.

## 11. After the first checks with the real app

The user ran S19 and S20 with the real 1Password app (op-reason's window, the toasts and 1Password's prompt all came
on the screen of the foreground window, which was not the primary one) and asked for two changes (2026-11-19):

- **70% opacity.** The text behind a toast could not be read. Every card, its face, text and lines, is drawn at 70%
  opacity from the moment it appears, so that the text behind shows through, and fades from 70% to nothing. Without
  motion it appears at 70% and goes at once.
- **`+N`.** Ten calls at once showed only three cards. The cards pushed out are counted until four seconds after their
  last signature, as if they were still shown, and `+N` under the third card says how many are left: N goes up when a
  card is pushed out and down when a pushed-out card's four seconds have passed, and the tag goes at 0. It counts cards
  (a call of ×5 is one).

Implementer's decisions:

- **Each card is drawn into a Direct2D layer** whose opacity is the card's (0.7 while shown, down to 0 as it fades):
  the card is one picture at 70%, not a face, text and lines each at 70% on top of each other (where the face would
  show through the text). The `+N` tag is drawn the same way, at 70%.
- **A bump keeps the card at 70%; its green ring is drawn at full strength**, after the card's layer and outside it,
  as before this change (it fades with the card only). Lifting the whole card to 100% at each bump was built first and
  dropped after the review: while a call signs again and again (ansible, every few hundred milliseconds) the card
  stayed nearly opaque (0.89 on average in the README's scene), and the text behind was hidden again. Without motion
  there is no bump, as before.
- **The card pushed out is the one whose last signature is the oldest**, no longer the bottom one, which may have
  signed again a moment ago. So the cards shown always signed after those pushed out: `+N` is back at 0 before a card
  shown starts to fade, and there is always a third card above it.
- **A pushed-out call that signs again comes back** as a new card at the top (pushing another card out) and is no
  longer counted in `+N`; its ×N goes on.
- **The tag**: 4 DIPs under the third place, at the cards' left edge, 16 DIPs high, light yellow with a gray border and
  `+N` in dark gray bold 11 DIPs (unlike ×N, green inside line 1). It appears, changes and goes at once. The toast
  window is 20 DIPs taller for it, and its line ends the window's text (`+2`).
- **The animation and the HTML page** add a fifth call to the scene of the fourth card, so that `+N` goes +1, +2, +1
  and away.
