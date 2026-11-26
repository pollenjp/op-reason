# The relay's process tree, and one ssh at a time — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The relay's windows show the processes from the caller to the client as a tree, with the box on the row of
its process. A call of Claude Code's PowerShell tool asks for each ssh, offering "Allow this ssh" and "Allow this
call".

**Architecture:**

- The relay already reads the client's chain (`peer::chain`) and finds the root (`roots::find`). It now hands the
  window the chain's rows (`Facts::tree`) and, for rule 3, `per_ssh`.
- The portable view (`look.rs`, `relay_ui.rs`) turns them into rows, the row of the box, small lines and frames, which
  `window.rs` draws.
- The book (`decide.rs`) remembers an answer for a client process next to those for calls and terminals.

**Tech Stack:** Rust (workspace `crates/op-reason-win` with `crates/op-reason-core`), the `windows` crate (Direct2D,
DirectWrite), PowerShell (the screenshots), Python (the README's animation).

**Spec:** `docs/superpowers/specs/2026-11-27-relay-process-tree-design.md`

## Global Constraints

- No new dependency and no new `windows` feature.
- The relay's windows keep Direct2D's software renderer, the window class and the points the tests click (§3 of
  `2026-11-26-modern-window-design.md`).
- Text 4.5:1 on its background in both palettes; the elbows (`tree` token) are not text.
- `OPREASON_SSH_CALL` v1 and the `op` payload unchanged; `ssh-sign` keeps `v` 1.
- No ticket ID, URL, personal value or reference to another repository in the repository's files, commits or pull
  request title.
- Linux checks: `nix develop path:./nix --command cargo test` at the root, and `nix develop path:../../nix --command
  cargo test` in `crates/op-reason-win` (with `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`).
  The Windows parts are checked by the pull request's CI only.

## Review Focus

1. A second `ssh.exe` of a call of the PowerShell tool while the first one's window asks: it must follow that window,
   then ask in its own after "Allow this ssh", and pass after "Allow this call".
2. A client whose start time is 0: "Allow this ssh" must not be remembered by PID alone.
3. A tree with no root (forwarded, unreadable, WSL direct): no tree and no frames, the box as today.
4. A long caller chain (more than four between) in a small work area: the fold must happen before the box is cut, and
   the buttons stay on screen.
5. Japanese texts with a long pill and a long frame name on one row: the frame's name must not cover the row's text
   (wrap or move it under).

---

### Task 1: The book, the records and the log

**Files:** `crates/op-reason-core/src/ssh/decide.rs`, `crates/op-reason-core/src/ssh/record.rs`,
`crates/op-reason-core/src/logview.rs` (tests in each).

**Interfaces (produces):**

- `Answer::AllowSsh`, `as_str` = `"allow-ssh"`.
- `Book::lookup(&mut self, mark: &Mark, client: Option<&ProcId>, shell: Option<&ProcId>, now: i64, alive: &dyn
  Fn(&ProcId) -> bool) -> Option<Decision>`.
- `Book::remember(&mut self, mark: &Mark, client: Option<&ProcId>, shell: Option<&ProcId>, answer: Answer, id: &str,
  now: i64)`.
- `Sign::by: Option<String>` (skipped when `None`).

- [ ] Tests: `an_answer_for_a_process_covers_that_process_only`, `an_answer_for_a_process_ends_with_it_or_at_the_limit`,
  `allow_ssh_is_not_remembered_without_a_client`, `the_call_answer_still_covers_every_process`, and the sweep counting
  the new map; a record with `by`; `--log` rows for `allow-ssh` (click and pass) and a pass of the terminal's answer.
- [ ] Run them (they fail), implement, run the core tests (pass), commit.

### Task 2: The portable view

**Files:** `crates/op-reason-win/src/look.rs`, `relay_ui.rs`, `ui.rs`, `theme.rs` (tests in each).

**Interfaces (produces):**

- `relay_ui::TreeProc { name: String, pid: Option<u32> }`.
- `relay_ui::Tree { head: Option<TreeProc>, between: Vec<TreeProc>, unseen: bool, client: Option<TreeProc> }`, where
  the head is `None` when the root is the caller.
- `Facts::tree: Option<Tree>`, `Facts::per_ssh: bool`.
- `look::Tree { rows: Vec<Row>, boxed: usize, small: Option<(usize, String)>, frames: Vec<Frame> }` on
  `View::tree: Option<look::Tree>`.
- `look::Row { depth: usize, dot: Option<ChipKind>, name: String, detail: String, pill: Option<String>, mark:
  Option<String>, tone: RowTone }`, `look::Frame { first: usize, last: usize, label: String, solid: bool }`.
- `look::fold(between: usize, limit: usize) -> ...` (the rows kept and the count folded).
- `ui::Button::AllowSsh`; `theme::Palette::tree`.

- [ ] Tests (both languages): rule 3 (title, buttons, frames, line); Claude Code's Bash tool (box at the root, the rest
  of the line, small ssh command at the leaf); a person with the terminal's frame; a program as the head; WSL unseen;
  no root; a deep tree folded; `Asking` without frames; `pass_texts` of `AllowSsh`; contrast of the new inks.
- [ ] Run (fail), implement, run `cargo test` in `crates/op-reason-win` (pass), commit.

### Task 3: The relay on Windows

**Files:** `crates/op-reason-win/src/win/relay/conn.rs`, `ask.rs`.

**Interfaces (consumes):** Task 1's book and `Sign::by`; Task 2's `Facts::tree`, `Facts::per_ssh`,
`Button::AllowSsh`.

- [ ] `identify` and `from_wsl` build `Tree` and keep the client's `ProcId` (start ≠ 0); `sign` sets `per_ssh`;
  `per_call` and `ask` pass the client to the book; `record` writes `by` for a covered request.
- [ ] `cargo clippy` on Linux for the portable parts; the pull request's CI for the rest.

### Task 4: The window

**Files:** `crates/op-reason-win/src/win/window.rs`.

- [ ] Draw the tree (rows, elbows, dots, pills, marks), the box under its row, the small lines, the frames (no room
  of their own); `Fit::FoldTree` before `Fit::CutCode`; the buttons styled by place.
- [ ] The pull request's CI (the tests that click; the screenshots).

### Task 5: Windows tests and screenshots

**Files:** `crates/op-reason-win/tests/relay.rs`, `.github/scripts/windows-exe-shots.ps1`.

- [ ] `ASK_SSH` for the title of rule 3; the launcher's script runs two `ssh.exe`; tests of §8 of the spec (two
  windows with "Allow this ssh"; "Allow this call" then `passed` with `by` `allow`; one ssh signing twice).
- [ ] Scenes: rule 3 in Japanese (exists) and in English, light; the others show their trees.

### Task 6: Documents and the README's animation

**Files:** `README.md`, `README.ja.md`, `skill/SKILL.md`, `docs/architecture/README.md`,
`docs/manual-verification.md`, `docs/manual-verification.ja.md`, `CHANGELOG.md`, `docs/images/make_readme_svgs.py`,
`docs/images/readme-6-relay-screens.*.svg`, `docs/html/relay-screens.html`.

- [ ] Texts per §9 of the spec; the animation draws the tree as `window.rs` does (its copied texts follow
  `relay_ui.rs`); `python3 docs/images/make_readme_svgs.py --check` passes.

### Task 7: Review, then the pull request

- [ ] A fresh reviewer reads the branch against the spec; findings and changes go to §13 of the spec.
- [ ] The pull request is made ready; the report goes to the conductor.
