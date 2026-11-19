# Toasts for signatures that pass, the screen of the windows, fewer notifications — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** per-call signatures that pass without a window get a toast at the top left of the screen in use (one card per
call); op-reason's windows open on the screen of the foreground window; a window's Windows notification goes with it.

**Architecture:** The cards' rules (one per call, three at most, time, counts) and the toast's texts are portable code in
`crates/op-reason-win` (`pass_toast`, `relay_ui`), tested on Linux. The Windows part adds the choice of the screen
(`win/screen.rs`), one layered window drawn with Direct2D on a UI thread of its own (`win/pass_window.rs`) that the
relay's signing threads feed through a channel, and tagged notifications that a worker thread removes when their window
closes (`win/toast.rs`).

**Tech Stack:** Rust 2024 (`crates/op-reason-win`, its own workspace), the `windows` crate 0.62.2 (no new crates, no new
features), Python 3 standard library (`docs/images/make_readme_svgs.py`), PowerShell 7 (the screenshot script), one
HTML file.

**Spec:** `docs/superpowers/specs/2026-11-18-pass-toasts-design.md`

**Execution:** inline (superpowers:executing-plans), one implementer: the Windows-only code is checked locally with
`cargo clippy --target x86_64-pc-windows-msvc` and run only by CI, and the portable model, the window and the documents
share the same constants and texts; a review of the whole branch at the end.

## Global Constraints

- Toast: 4 s shown after the last signature, 0.8 s fade, slide in and move 240 ms, appear 220 ms, pushed out 140 ms, bump
  420 ms, at most 3 cards; 12 DIPs from the work area's edges, 8 DIPs between cards; card 300 × 70 DIPs (spec §4.4,
  §10).
- Only per-call, only `Outcome::Passed` with a signature from 1Password, and not after a late window was shown (§4.1).
- Animation effects off (`SPI_GETCLIENTAREAANIMATION`): no slide, fade, bump or line; cards appear and go at once.
- Notifications: tag `<pid>-<n>`, group `op-reason`, AppUserModelID unchanged (Windows PowerShell's); removed on every
  close of a window except a per-call time-out; one-time notices never tagged (§6, §10).
- Windows open on the foreground window's screen (`GetForegroundWindow` → `MonitorFromWindow` → `rcWork`, its DPI),
  else the primary one; they never move after opening (§5).
- Records keep their shape; Linux code unchanged; no version bump; no new crates; texts in English and Japanese.

## Review Focus

1. ansible's forks pass dozens of signatures of one call at once: one card, ×N right, and a signing thread never waits
   for the UI thread (Task 5: `send` and an event, nothing blocks; Task 2: `a_burst_of_one_call_is_one_card`).
2. No toast window can be made (no interactive desktop, a failed Direct2D factory): the request is answered and
   recorded as before, and nothing panics (Task 5: `Toasts::start` returns `None`; `pass` ignores a closed channel).
3. A window that closes within milliseconds removes its notification, which was not shown yet (Task 4: the worker shows
   first, then reads the message; Windows CI `the_window_notification_goes_with_the_window`).
4. A relay that runs for days: no thread or count piles up (Task 4: a notification's thread ends at `hide` or when its
   `Pending` drops; Task 2: `counts_unused_for_the_time_limit_are_forgotten`).
5. A command line of 300 characters, a destination of 200, Japanese callers: the card stays 70 DIPs high and the
   destination stays visible (Task 5: uniform line spacing, the command trimmed at the width left by the destination;
   the CI screenshot shows a long one).

---

### Task 1: The toast's texts (portable)

**Files:**
- Modify: `crates/op-reason-win/src/relay_ui.rs` (table entries, `PassTexts`, `pass_texts`, tests)

**Interfaces:**
- Produces:
  - `pub struct PassTexts { pub word: String, pub caller: String, pub command: String, pub destination: Option<String>, pub why: String }` (`Debug, Clone, PartialEq, Eq`)
  - `impl PassTexts { pub fn head(&self, count: u32) -> String; pub fn line2(&self) -> String }` — `head(1)` = `"Passed · Claude Code"`, `head(3)` = `"Passed · Claude Code ×3"`; `line2` = `"<command> → <destination>"`, the command alone without one.
  - `pub fn pass_texts(f: &Facts, answer: Answer, lang: Lang) -> PassTexts` (`Answer` from `op_reason_core::ssh::decide`).
- Table: `pass_word` "Passed" / 「通過」, `pass_call` "allowed for this call" / 「この呼び出しの許可」, `pass_terminal`
  "allowed for this terminal" / 「ターミナルの許可」; the reason or `no_reason`.

- [ ] **Step 1: Write the failing tests** `the_pass_toast_says_who_what_and_why` (Claude Code, `Answer::Allow`, English:
  word "Passed", caller "Claude Code", command and destination of `facts()`, why "allowed for this call · push the fix";
  `head(1)`, `head(3)` and `line2()` as above; Japanese: 「通過」, `head(2)` 「通過 · Claude Code ×2」, why
  「この呼び出しの許可 · push the fix」) and `the_terminal_answer_and_no_reason` (`Answer::AllowTerminal`, a person, empty
  reason, no destination: why "allowed for this terminal · no reason given" / 「ターミナルの許可 · 理由なし」, caller
  "Human" / 「人」, `line2()` is the command alone).
- [ ] **Step 2:** `nix develop path:../../nix --command cargo test -p op-reason-win relay_ui` in `crates/op-reason-win`:
  fails (no `pass_texts`).
- [ ] **Step 3:** implement; `caller` from `caller_short`, `destination` from `f.destination` (empty is `None`).
- [ ] **Step 4:** the same command passes; `cargo clippy --locked --workspace --all-targets -- -D warnings` passes.
- [ ] **Step 5:** commit `feat(relay): the texts of the toast of a signature that passed`.

### Task 2: The cards (portable)

**Files:**
- Create: `crates/op-reason-win/src/pass_toast.rs`
- Modify: `crates/op-reason-win/src/lib.rs` (`pub mod pass_toast;`, the crate doc)

**Interfaces:**
- Consumes: `relay_ui::PassTexts`.
- Produces (times are milliseconds on the caller's clock, lengths DIPs):
  - constants `SHOW_MS = 4000`, `FADE_MS = 800`, `ENTER_MS = 240`, `APPEAR_MS = 220`, `EVICT_MS = 140`, `BUMP_MS = 420`,
    `FRAME_MS = 16`, `MAX_CARDS = 3`, `EDGE = 12.0`, `GAP = 8.0`, `CARD_W = 300.0`, `CARD_H = 70.0`, `DROP = 12.0`.
  - `pub struct Stack<K>`; `impl<K: Clone + Eq + Hash> Stack<K>`: `new(animate: bool, forget_ms: u64) -> Self`,
    `set_animate(&mut self, animate: bool)`, `is_empty(&self) -> bool`,
    `pass(&mut self, now: u64, call: K, answer: &str, texts: PassTexts)`, `advance(&mut self, now: u64)`,
    `frame(&self, now: u64) -> Vec<CardView<'_>>` (top first, then cards that are leaving),
    `next_wake(&self, now: u64) -> Option<u64>`, `heads(&self) -> Vec<String>` (each shown card's `head`, newest first).
  - `pub struct CardView<'a> { pub id: u64, pub y: f32, pub opacity: f32, pub glow: f32, pub left: Option<f32>, pub count: u32, pub texts: &'a PassTexts }`
    — `y` is the card's top from the stack's top (slot `i` rests at `EDGE + i * (CARD_H + GAP)`), `glow` 0..1, `left`
    the share of the 4 s left while the card is live (none when animation is off or it is fading).
- Rules (spec §4.4, from the motion page): a new card enters at the top from `DROP` above its slot (ease-out over
  `ENTER_MS`, opacity 0→1 over `APPEAR_MS`); the others move to their new slots over `ENTER_MS`; a fourth card pushes out
  the oldest at once (opacity to 0 over `EVICT_MS`, gone 40 ms later, no longer holding a slot); a pass of a shown call
  adds one, takes the new texts, glows (`BUMP_MS`, peak at a quarter) and starts `SHOW_MS` again, and a fading card
  comes back to full opacity over `APPEAR_MS`; at `SHOW_MS` after its last pass a card fades over `FADE_MS`, then frees its
  slot and the cards below move up. Transitions happen at their own times even when `advance` comes later. Counts are
  kept per `(call, answer)` and forgotten after `forget_ms` without a pass.

- [ ] **Step 1: Write the failing tests**: `a_new_card_slides_in_at_the_top_and_pushes_the_others_down` (three calls at
  0, 1000, 2000: order newest first, slots at 12, 90, 168 once settled, the newest at 0 ms of its entry at
  `12 - DROP` with opacity 0); `one_card_per_call_counts_and_shows_the_last_line` (three passes of one call: one card,
  count 3, the last texts, glow > 0 right after a pass, `heads()` = `["Passed · Human ×3"]`-style);
  `a_burst_of_one_call_is_one_card` (fifty passes at the same millisecond: one card, count 50);
  `four_seconds_after_the_last_pass_it_fades_and_the_others_close_up` (card alive at 3999, opacity < 1 at 4400,
  gone at 4800, the card below rests in slot 0 after `ENTER_MS` more); `a_pass_brings_a_fading_card_back` (pass at
  4300: opacity back to 1 by 4520, still shown at 8200, gone at 9100); `the_fourth_card_pushes_the_oldest_out_at_once`
  (four calls: three hold slots, the oldest leaving with opacity 0 by 140 ms and gone from `frame` by 180 ms);
  `the_count_goes_on_after_the_card_has_gone_and_starts_again_for_a_new_answer` (pass, wait 5 s, pass: count 2; same call
  with another answer ID: count 1); `counts_unused_for_the_time_limit_are_forgotten` (`forget_ms` 1000: a pass after
  1500 ms counts 1); `without_animation_cards_appear_and_go_at_once` (opacity 1 and the slot at once, no glow, `left` none,
  gone exactly at `SHOW_MS`); `when_to_draw_next` (empty: `None`; animated card: `now + FRAME_MS`; animation off:
  the time it goes).
- [ ] **Step 2:** `cargo test -p op-reason-win pass_toast` fails (no module).
- [ ] **Step 3:** implement `pass_toast.rs` (cards in display order plus a list of leaving cards; a small
  `Anim { from, to, at, dur, ease }` evaluated at `now`; `advance` applies due transitions in time order).
- [ ] **Step 4:** tests and clippy pass (Linux).
- [ ] **Step 5:** commit `feat(relay): the cards of the toasts: one per call, three at most, four seconds`.

### Task 3: A — the screen of the windows

**Files:**
- Create: `crates/op-reason-win/src/win/screen.rs`
- Modify: `crates/op-reason-win/src/win/mod.rs`, `crates/op-reason-win/src/win/window.rs` (`Window::create`, the module
  doc; `locale` and `format` become `pub(super)` for Task 5)

**Interfaces:**
- Produces: `pub struct Screen { pub work: RECT, pub dpi: u32 }` (`Clone, Copy`) and `pub fn of_foreground() -> Screen`:
  `GetForegroundWindow`; a null window or a failing `GetMonitorInfoW` falls back to the primary monitor
  (`MonitorFromPoint(0, 0, MONITOR_DEFAULTTOPRIMARY)`), and a failing monitor query to `SPI_GETWORKAREA`; DPI by
  `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`, 96 when it fails.

- [ ] **Step 1:** implement `screen.rs`; `Window::create` takes `dpi` and `work` from `screen::of_foreground()` instead
  of `MonitorFromPoint` and `SPI_GETWORKAREA`.
- [ ] **Step 2:** `nix develop path:./nix --command bash <scratchpad>/wincargo.sh crates/op-reason-win clippy --locked
  --target x86_64-pc-windows-msvc --workspace --all-targets -- -D warnings` passes (rustup's 1.99.1 with the Windows
  target, the dev shell's C compiler for build scripts).
- [ ] **Step 3:** commit `feat(win): windows open on the screen of the foreground window`.

### Task 4: B — a window's notification goes with it

**Files:**
- Modify: `crates/op-reason-win/src/win/toast.rs`, `crates/op-reason-win/src/win/relay/ask.rs` (`Shown`, `ask`),
  `crates/op-reason-win/src/win/app.rs` (`run_desktop`)

**Interfaces:**
- Produces: `pub fn start_for_window(lines: Vec<String>) -> Pending` (tag and group set), `Pending::hide(&mut self)`
  (removes it once shown: `ToastNotifier::Hide` and `ToastNotificationHistory::RemoveGroupedTagWithId(tag, "op-reason",
  AUMID)` on the worker thread), `pub const GROUP: &str = "op-reason"`, `pub const AUMID` made `pub`; `start` and
  `Pending::result` unchanged for the one-time notices.
- `Shown::keep_notification(&mut self)`: called by `ask` at the per-call time-out; `Drop` hides otherwise.
- `run_desktop`: `start_for_window`; after each `drop(win)` the notification is hidden when a window was shown.

- [ ] **Step 1:** implement (the worker sends the result first, then waits on a second channel: a message removes, a
  dropped sender keeps).
- [ ] **Step 2:** the Windows clippy of Task 3 passes; Linux tests still pass.
- [ ] **Step 3:** commit `feat(win): a window's notification goes when the window closes`.

### Task 5: The toast window, and the relay's toasts

**Files:**
- Create: `crates/op-reason-win/src/win/pass_window.rs`
- Modify: `crates/op-reason-win/src/win/mod.rs`, `crates/op-reason-win/src/win/relay/mod.rs` (`Shared::toasts`),
  `crates/op-reason-win/src/win/relay/conn.rs` (`exchange` reports a shown late window; `sign` sends the toast)

**Interfaces:**
- Consumes: `pass_toast::Stack`, `relay_ui::{PassTexts, pass_texts}`, `screen::of_foreground`, `window::{locale, format}`.
- Produces: `pub struct Toasts`; `Toasts::start(forget: Duration) -> Option<Toasts>` (a thread named
  `op-reason-toasts`, an auto-reset event); `Toasts::pass(&self, call: Mark, answer: &str, texts: PassTexts)`.
  The window: class `op-reason-pass-toast`, `WS_POPUP`, `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE |
  WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, `WM_NCHITTEST` → `HTTRANSPARENT`, `WM_MOUSEACTIVATE` → `MA_NOACTIVATE`; its text is
  `Stack::heads` joined by newlines.
- `Shared::toasts(&self) -> Option<&Toasts>` (a `OnceLock`, started with `opts.limit_secs`).
- `conn.rs`: `struct Exchanged { reply: Reply, secs: f64, late_window: bool }` returned by `exchange`.

- [ ] **Step 1:** `pass_window.rs`: the loop (`MsgWaitForMultipleObjectsEx` on the event with `next_wake`, then pump,
  drain the channel, `advance`, draw); the first card of an empty stack takes `screen::of_foreground()` and
  `SPI_GETCLIENTAREAANIMATION`; drawing: a 32-bit top-down DIB in a memory DC, a Direct2D DC render target (premultiplied
  BGRA, the screen's DPI, grayscale text), `UpdateLayeredWindow(ULW_ALPHA)` at the work area's top left, shown with
  `SW_SHOWNOACTIVATE` and put topmost without activation, hidden when empty. A card: light yellow rounded rectangle with
  a gray border, a green check, line 1 (`word` green bold, ` · caller` bold, a green `×N` tag), line 2 (the command
  trimmed at the width left by ` → destination`), line 3 gray 11 DIPs, the line of the time left, the glow ring; uniform
  line spacing (17, 17, 16 DIPs; padding 9 and 11).
- [ ] **Step 2:** `Shared::toasts`; `sign`'s `Passed` branch builds `pass_texts(&facts, d.answer, shared.lang())`
  before the exchange and sends it when 1Password signed and no late window was shown.
- [ ] **Step 3:** the Windows clippy passes; Linux tests pass.
- [ ] **Step 4:** commit `feat(relay): a toast at the top left for a signature that passes without a window`.

### Task 6: Windows CI — tests and screenshots

**Files:**
- Modify: `crates/op-reason-win/tests/relay.rs`, `crates/op-reason-win/tests/exe.rs`,
  `.github/scripts/windows-exe-shots.ps1`

- [ ] **Step 1:** `relay.rs`: `a_signature_that_passes_without_a_window_shows_a_toast` (CI only: Allow, then the same
  call `sign sign sign`; the window of class `op-reason-pass-toast` is visible, its text has "×3", its extended style
  has the five flags, `WindowFromPoint` at its middle is not it, the foreground window is the same as before, its top
  left is the work area's of the foreground window's screen; hidden within 8 s); `the_window_notification_goes_with_the_window`
  (CI only: while the window asks, the history of the AppUserModelID has an `op-reason` notification; after Deny it has
  none; when the history is empty while the window is open, say so and return); `a_timed_out_window_leaves_its_notification`
  (no click: `--answer-wait 3s`, the `op-reason` notification is there after the time-out, then removed by the test).
- [ ] **Step 2:** `exe.rs`: `a_desktop_call_removes_its_notification` (with `FAKE_SLEEP_MS` 3000: seen while op runs when
  the history works, gone after the call).
- [ ] **Step 3:** `windows-exe-shots.ps1`: a relay scene in Japanese: two calls allowed with a click each, then three
  signatures of the first and one of the second; the toast window (by class, captured with `CaptureBlt`) as
  `16-relay-toasts-ja.png` and the whole screen as `17-relay-toasts-screen.png`.
- [ ] **Step 4:** the Windows clippy passes (tests included); commit `test(win): the toasts, the notifications removed,
  and their screenshots`.

### Task 7: The README animation and the HTML page

**Files:**
- Modify: `docs/images/make_readme_svgs.py` (copies of the toast's texts and values, `scene_relay`, `--check` of the HTML
  page)
- Create: `docs/images/readme-6-relay-screens.en.svg`, `docs/images/readme-6-relay-screens.ja.svg` (generated),
  `docs/html/relay-screens.html`

- [ ] **Step 1:** `make_readme_svgs.py`: copies of `pass_word`, `pass_call`, `pass_terminal`, the relay window's title
  and buttons (`relay_ui.rs`) and of the constants of `pass_toast.rs`, checked in `sources()`; `scene_relay(lang)`: a
  desktop with the relay window at the top right, toasts at the top left, the notifications' corner at the bottom right;
  steps: a toast comes and fades, one call's card counts up (×N, glow), other calls push in and the fourth pushes out the
  oldest, the window asks with its notification, Allow closes both and the call's signatures come as a toast; the rest
  frame shows the window, a toast and the notification together; captions; `<title>` and `<desc>` as the README alt.
- [ ] **Step 2:** `docs/html/relay-screens.html` from the motion page: both languages (`#ja`), the implementation's
  values and texts, pause / restart / slow / scene rail, reduced motion, external links `https` with
  `target="_blank" rel="noopener"`; the script's `--check` fails when the page lacks a copied text or value.
- [ ] **Step 3:** `python3 docs/images/make_readme_svgs.py` then `--check` pass; frames captured with headless
  Chromium (light page, Japanese fonts, `--force-prefers-reduced-motion`) and looked at; the HTML page captured in both
  languages and at 400 px wide, its links checked with the console snippet.
- [ ] **Step 4:** commit `docs: the animation and the HTML page of the relay's windows, toasts and notifications`.

### Task 8: The documents

**Files:**
- Modify: `README.md`, `README.ja.md` (subsection in "SSH keys: the relay", the design link in Development),
  `docs/README.md`, `docs/plantuml/01_map.puml` and `docs/plantuml/out/01_map.svg`, `docs/manual-verification.md`,
  `docs/manual-verification.ja.md` (S19–S24), `docs/architecture/README.md`, `CHANGELOG.md` (Unreleased)

- [ ] **Step 1:** write them (the README alt equal to the SVG `<desc>`; the map regenerated with the PlantUML skill's
  flake).
- [ ] **Step 2:** `bash tests/run-all.sh` is untouched by this (no Linux change); `python3 docs/images/make_readme_svgs.py
  --check` passes.
- [ ] **Step 3:** commit `docs: the toasts, the screen of the windows and the notifications`.

### Task 9: CI and the pull request

- [ ] **Step 1:** push the branch, open the pull request (no ticket ID or URL: public repository), watch CI; fix what
  fails on Windows.
- [ ] **Step 2:** check that the run keeps the artifact `op-reason-windows` and that the screenshots comment shows the
  toasts; review the whole branch.

---

## After the first checks with the real app (spec §11)

The user asked for two changes after S19 and S20: every card at 70% opacity, and `+N` under the third card for the
cards pushed out. Same execution (inline, one implementer), on the same branch and pull request; the version stays.

### Review Focus (§11)

6. `+N` never stands under fewer than three cards and is back at 0 before a card shown starts to fade: the card pushed
   out is the one whose last signature is the oldest (Task 10:
   `the_card_pushed_out_is_the_one_whose_last_signature_is_the_oldest`, `the_count_is_gone_before_a_card_shown_fades`).
7. The text behind a card shows through at 30% everywhere on the card, the fade included: one Direct2D layer per card,
   never a face, text and lines each at 70% (Task 11; the CI screenshot).
8. Ten calls at once: `+7`, going down as each pushed-out card's four seconds pass; a pushed-out call that signs again
   comes back on top (Task 10: `cards_pushed_out_are_counted_until_their_four_seconds_pass`,
   `a_call_pushed_out_that_signs_again_comes_back_on_top`).

### Task 10: The cards: 70% and `+N` (portable)

**Files:**
- Modify: `crates/op-reason-win/src/pass_toast.rs`

**Interfaces:**
- `pub const OPACITY: f32 = 0.7;` (a card shown), `pub const MORE_GAP: f32 = 4.0;`, `pub const MORE_H: f32 = 16.0;`,
  `pub const fn more_y() -> f32` (the top of the `+N` tag: under the third place).
- `CardView::opacity` = how far the card has appeared (0 to 1, as before) × `OPACITY`; `CardView::glow` = the glow ×
  how far the card has appeared (its ring, drawn at full strength outside the card's layer).
- `Stack::more(&self) -> usize`: the cards pushed out whose four seconds have not passed (call `advance` first).
- The card pushed out: the one whose last signature is the oldest (a tie: the lower one). A pushed-out call that signs
  again enters at the top as a new card and leaves the count. Without motion, `next_wake` also wakes at the end of a
  pushed-out card's four seconds.

- [ ] **Step 1:** failing tests: `a_card_is_drawn_at_seventy_percent` (in place, halfway through its fade, without
  motion), `a_bump_rings_a_card_that_stays_at_seventy_percent`,
  `cards_pushed_out_are_counted_until_their_four_seconds_pass` (ten calls: +7, then down one by one, cards counted once
  whatever their ×N), `the_card_pushed_out_is_the_one_whose_last_signature_is_the_oldest`,
  `of_cards_that_signed_at_once_the_lowest_is_pushed_out`,
  `a_call_pushed_out_that_signs_again_comes_back_on_top`, `the_count_is_gone_before_a_card_shown_fades`,
  `without_animation_the_count_goes_down_on_time`; the existing tests read 0.7 where they read 1.
- [ ] **Step 2:** implement; `cargo test` and `cargo clippy --all-targets -- -D warnings` in `crates/op-reason-win`.

### Task 11: The toast window: layers and the tag (Windows)

**Files:**
- Modify: `crates/op-reason-win/src/win/pass_window.rs`

- [ ] **Step 1:** one `ID2D1Layer` per surface; each card is drawn inside `PushLayer` with its opacity (content bounds:
  the card with its shadow), its brushes at their own alphas; then, outside the layer, the ring of its glow (the glow
  brush's opacity is `CardView::glow`).
- [ ] **Step 2:** the tag `+N` at (`EDGE`, `more_y()`), `MORE_H` high, light yellow, gray border, dark gray bold 11
  DIPs, in a layer at `OPACITY`, laid out again only when N changes; the bitmap `STACK_H` = `more_y()` + `MORE_H` +
  `EDGE`; the window's text: the cards' first lines, then `+N`.
- [ ] **Step 3:** `cargo clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings` (the scratchpad's
  `wincargo.sh`).

### Task 12: Windows CI: `+N` and the screenshot

**Files:**
- Modify: `crates/op-reason-win/tests/relay.rs`, `.github/scripts/windows-exe-shots.ps1`

- [ ] **Step 1:** `a_fourth_call_pushes_a_card_out_and_counts_it` (CI only): four reasons allowed one by one, then each
  signs once without a window: three first lines and `+1` in the window's text.
- [ ] **Step 2:** the screenshot scene: three calls allowed and passed first (two of them are pushed out), then the call
  of three signatures and the call with the long destination: three cards and `+2`.

### Task 13: The README animation and the HTML page

**Files:**
- Modify: `docs/images/make_readme_svgs.py`, `docs/images/readme-6-relay-screens.{en,ja}.svg` (generated),
  `docs/html/relay-screens.html`

- [ ] **Step 1:** the script: copies of `OPACITY`, `MORE_GAP`, `MORE_H` (checked); `simulate_toasts` pushes out the card
  whose last signature is the oldest and keeps the counts of `+N` over time; a card's group fades, an inner group holds
  it at 0.7 and the ring of its glow is outside that group; the tag under the third place, one per value of N; the
  scene's fourth card at 16.0 s and
  a fifth call at 16.4 s (+1, +2, +1, gone), the caption at 15.8 s; `relay.desc` and the fourth step say `+N`.
- [ ] **Step 2:** the HTML page: the same rules, the tag, the fifth call, its callouts and scenes; `OPACITY = 0.7` in its
  constants (checked).
- [ ] **Step 3:** generate, `--check`; frames of the SVG and the page looked at.

### Task 14: The documents, CI and the pull request

**Files:**
- Modify: `README.md`, `README.ja.md`, `docs/manual-verification.md`, `docs/manual-verification.ja.md`, `CHANGELOG.md`

- [ ] **Step 1:** the toasts' bullet (70%, `+N`), the alt equal to the new `<desc>`; S19 (the text behind shows
  through), S20 (`+1`), ten calls at once (`+7`, going down), S21 (70%, `+N` without motion); the Unreleased section.
- [ ] **Step 2:** push, CI green, the screenshot looked at, the pull request's body updated; report.
