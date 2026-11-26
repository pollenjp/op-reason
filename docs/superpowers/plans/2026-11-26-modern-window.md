# A modern look for the windows — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `op-reason.exe`'s windows (an `op` call's, the relay's in both phases) and the relay's toasts drawn in the
approved direction, in Windows' light or dark app mode, with the relay's line about how long an answer lasts tied to
the root command.

**Architecture:** Two portable modules carry every decision that can be tested on Linux: `theme.rs` (the palettes) and
`look.rs` (the `View` the window draws and the helpers that build it: colored words, the root command, the name of the
line about how long an answer lasts, marked words, chip rows, the switch's pill). `ui.rs` and `relay_ui.rs` build a
`View` per language; `win/window.rs` lays it out and draws it with Direct2D; `win/theme.rs` reads the app mode and sets
the frame's attributes.

**Tech Stack:** Rust 1.99.1 (`crates/op-reason-win`, a Cargo workspace of its own), `windows` 0.62.2 (features
`Win32_Graphics_Dwm` and `Win32_System_Registry` added), Direct2D/DirectWrite, PowerShell 7 (the screenshot script),
Python 3 (the README animations), GitHub Actions (`windows-2025`).

**Spec:** `docs/superpowers/specs/2026-11-26-modern-window-design.md`

## Global Constraints

- No new crate; no bitmap; no copy or screenshot of 1Password's screens in the repository.
- The window keeps `WS_EX_NOACTIVATE`, `MA_NOACTIVATE`, no default button, the class `op-reason-approval`, the window
  text (the title), 520 DIPs outer width, the top right of the foreground window's screen, 16 DIPs from the edges.
- The points the tests click: buttons 20 DIPs inside the client area's left/right edge and 26 above its bottom;
  日本語 26 DIPs left of the right edge and 21 above the bottom (an `op` call's window); links at 40 DIPs from the left.
- The relay's windows draw on the software DC render target (`draw_without_gpu`); an `op` call's keeps the default.
- `launcher/launcher.ps1` and `ui::Table` (the texts `Get-Texts` shares) unchanged; `Texts::json()` (the status `ui`)
  unchanged.
- Colors exactly as the spec's §4.4 table; texts exactly as its §5 table.
- Every text color meets 4.5:1 on its background in both palettes.
- Public repository: no ticket IDs, user names, host names or local paths in files, commits or the pull request; example
  values only from the repository's tests (pid 4242, `w1:p2`, `me/r`, `example/infra`, the made-up `op://` IDs).
- Lints as CI runs them: in `crates/op-reason-win`, `cargo fmt --check -p op-reason-win -p fake-op`,
  `cargo clippy --locked --workspace --all-targets -- -D warnings` on Linux and for `x86_64-pc-windows-msvc`;
  `cargo test --locked --workspace` on Linux; the main workspace's fmt/clippy/test;
  `python3 docs/images/make_readme_svgs.py --check`.
- READMEs in English and Japanese change together; `CHANGELOG.md` under Unreleased.

## Review Focus

- A reason, a command or a caller detail far longer than the window (the screenshots' template path, the runner's
  process chain): wrapped, never cut, never over the switch or the buttons — `look::flow` tests (Task 2) and the
  screenshot scene `05-exe-long-en` (Task 7).
- A root command line cut at 200 characters, or with an unclosed quote, or a Claude Code wrapper followed by more
  commands: shown whole, never shortened to a part — `look::root_command` tests (Task 2).
- The theme changed while a window is open (`WM_SETTINGCHANGE`): colors change, layout and click targets do not —
  manual check (Task 8) and the code path in Task 5.
- A relay request without a root (forwarded, unknown caller, single): "Command" and the ssh command in the box, no
  empty "Root command" — `relay_ui` test `a_request_without_a_root_shows_the_ssh_command` (Task 4).
- DPI other than 96 (the window opens on the foreground window's screen): the click points scale — the existing CI
  tests click in DIPs; manual check at 150% (Task 8).

---

### Task 1: The palettes and the app mode

**Files:**
- Create: `crates/op-reason-win/src/theme.rs`
- Create: `crates/op-reason-win/src/win/theme.rs`
- Modify: `crates/op-reason-win/src/lib.rs` (module), `crates/op-reason-win/src/win/mod.rs` (module),
  `crates/op-reason-win/Cargo.toml` (features `Win32_Graphics_Dwm`, `Win32_System_Registry`)

**Interfaces:**
- Produces: `theme::Theme { Dark, Light }`, `Theme::from_apps_use_light_theme(Option<u32>) -> Theme`,
  `theme::Rgb(u8, u8, u8)` with `const fn hex(u32) -> Rgb`, `fn luminance(self) -> f64`, `theme::contrast(Rgb, Rgb) -> f64`,
  `theme::Palette` (fields: `background, text, secondary, tertiary, surface, line, code_bg, code_border, code_text,
  code_path, accent, on_accent, caution, on_caution, critical, orange, success, on_success, success_tint, claude,
  band_bg, band_border, band_title, band_text, pic_screen, pic_frame, pic_taskbar, pic_stand, pic_window, switch_shown`),
  `theme::palette(Theme) -> &'static Palette`, `theme::TEXT_PAIRS: &[(&str, fn(&Palette) -> (Rgb, Rgb))]` (every text
  color on its background, for the contrast test).
- Produces (Windows): `win::theme::current() -> Theme` (the registry value), `win::theme::frame(hwnd: HWND, theme: Theme)`
  (§4.1 attributes, failures ignored), `win::theme::changed(lparam: LPARAM) -> bool` (`WM_SETTINGCHANGE` with
  `ImmersiveColorSet`).

- [ ] **Step 1: Tests in `theme.rs`**: `dark_and_light_from_the_registry_value` (0 → Dark; 1, 2, None → Light);
  `every_text_color_meets_aa_on_its_background` (each `TEXT_PAIRS` entry ≥ 4.5 in both palettes, the message names the
  pair and the theme); `the_spec_colors` (dark `background` `#202020`, `accent` `#60cdff`, `caution` `#fce100`,
  `critical` `#ff99a4`; light `background` `#f9f9f9`, `accent` `#005fb8`, `caution` `#9d5d00`, `critical` `#c42b1c`).
- [ ] **Step 2: Run** `cargo test -p op-reason-win theme` in `crates/op-reason-win` → fails (no module).
- [ ] **Step 3: Implement** the palettes from the spec's §4.4 (`on_caution`: dark `#202020`, light `#ffffff`;
  `on_success`: dark `#000000`, light `#ffffff`; `success_tint`: dark `#1f3a1c`, light `#dff6dd`), the luminance and
  contrast of WCAG 2.x, and `win/theme.rs` (`RegGetValueW` with `RRF_RT_REG_DWORD`; `DwmSetWindowAttribute` for
  `DWMWA_USE_IMMERSIVE_DARK_MODE`, `DWMWA_CAPTION_COLOR` (`COLORREF` of `background`), `DWMWA_WINDOW_CORNER_PREFERENCE`;
  `SetWindowThemeAttribute(WTA_NONCLIENT, WTNCA_NODRAWCAPTION | WTNCA_NODRAWICON)`).
- [ ] **Step 4: Run** the tests → pass; `cargo clippy` on Linux and for the Windows target → clean.
- [ ] **Step 5: Commit** `feat(win): the palettes of the windows, and Windows' app mode`.

### Task 2: The portable helpers of the look

**Files:**
- Create: `crates/op-reason-win/src/look.rs`
- Modify: `crates/op-reason-win/src/lib.rs`

**Interfaces:**
- Produces: the `View` types (`Icon { Lock, Key }`, `Reason { Given(String), Missing(String) }`,
  `ChipKind { Claude, Person, Other, Herdr, Wsl, Repo, Folder }`, `Chip { kind, text }`,
  `FactKind { Purpose, Detail, Mono }`, `Fact { kind, text }`, `Sketch { Op, Ask, Asking }`,
  `Band { title: String, line: String, sketch: Sketch }` (marked text), `View { icon, subtitle, time, date,
  reason_label, reason, chips, code_label, code, code_note: Option<String>, facts, notes, countdown: Option<String>,
  status, band, footer }`), all `Debug, Clone, PartialEq, Eq`.
- Produces: `split_time(&str) -> (String, String)`; `Tone { Plain, Accent, Caution }`,
  `Marked { text: String, spans: Vec<(u32, u32, Tone)> }`, `marked(&str) -> Marked` (marks `⟦…⟧` accent, `⟪…⟫` caution;
  spans in UTF-16 units of `text`); `Word { Program, Path, Plain }`, `color_words(&str) -> Vec<(u32, u32, Word)>`
  (UTF-16 ranges, §4.6); `root_command(&str) -> Option<String>` (§4.7: `Some` only when a wrapper came off);
  `program_name(&str) -> Option<String>` (§4.8); `flow(widths: &[f32], row: f32, gap: f32) -> Vec<(usize, f32)>` (row
  index and x of each item; an item wider than `row` gets a row of its own at x 0); `pill(widths: [f32; 2], pad: f32,
  right: f32, top: f32, height: f32) -> [ui::Rect; 2]` and `pill_hit(&[ui::Rect; 2], shown: Lang, x: f32, y: f32) ->
  Option<Lang>`.

- [ ] **Step 1: Tests in `look.rs`**:
  - `split_time("2026-11-15 06:05:00") == ("06:05:00", "2026-11-15")`; anything else → `(whole, "")`.
  - `marked("ふつう⟦画面の中央⟧に出ます")`: text without marks, one Accent span over 画面の中央 (UTF-16 offsets);
    `⟪…⟫` Caution; no marks → no spans; an unclosed mark is plain text.
  - `color_words("op inject -i app.env.tpl -o .env")`: `op inject` Program, `-i`/`-o` Plain, `app.env.tpl`/`.env` Path;
    `git push origin fix/login`: `git push` Program, `origin`/`fix/login` Plain;
    `ssh.exe git@github.com "git-receive-pack 'me/r.git'"`: `ssh.exe` Program, both others Path;
    `op read op://Private/proxmox/token`: the reference Path.
  - `root_command`: each `rule: claude` root of `tests/cases/roots.json` (`/bin/bash -c -l "source /tmp/claude-shell-snapshot && eval 'git push'"` → `git push`;
    `C:\WINDOWS\system32\cmd.exe /d /s /c "pwsh -NoProfile -NonInteractive -Command ""ssh-add -l"""` (as `cmdline::shown`
    displays it) → `ssh-add -l`; `"C:\Program Files\Git\usr\bin\bash.exe" -c -l "ssh host"` → `ssh host`; `SSH.EXE host` → `None`);
    the screenshots' `bash -c -l "git push origin fix/login"` → `git push origin fix/login`; Claude Code's full form
    `/bin/bash -c -l "source ~/.claude/shell-snapshots/s.sh && eval 'git push origin main' \< /dev/null && pwd -P >| /tmp/c-cwd"` → `git push origin main`;
    `None` for: `bash -c "x" extra`, `bash -c "source a && eval 'x' && rm y"`, a line ending in `…`, an unclosed quote,
    `git pull --rebase`.
  - `program_name`: `git push origin fix/login` → `git push`; `ssh-add -l` → `ssh-add`; `C:\x\ssh.exe -T host` → `ssh`;
    `cd a && git push` → `None`; `""` → `None`.
  - `flow(&[100.0, 120.0, 200.0], 300.0, 6.0) == [(0, 0.0), (0, 106.0), (1, 0.0)]`; an item of 400 in a row of 300 →
    a row of its own at 0.
  - `pill([40.0, 33.0], 9.0, 500.0, 100.0, 24.0)`: Ja's rect ends at 500 and is 51 wide, En's ends where Ja's starts;
    `pill_hit` gives the language not shown inside its rect, `None` on the shown half and outside.
- [ ] **Step 2: Run** `cargo test -p op-reason-win look` → fails.
- [ ] **Step 3: Implement.** The command's words split as `cmdline::display` joined them (spaces; `"…"` with `\"`).
  `root_command` repeats the shell rule of §4.7 and then the wrapper rule once; it returns `None` when nothing came off.
- [ ] **Step 4: Run** → pass; clippy clean.
- [ ] **Step 5: Commit** `feat(win): the look's portable parts (colored words, the root command, chips, the switch)`.

### Task 3: The `View` of an `op` call

**Files:**
- Modify: `crates/op-reason-win/src/ui.rs`

**Interfaces:**
- Consumes: Task 2's `View` and helpers.
- Produces: `Texts.view: look::View`; `ui::Words` (the exe-only texts of §5: `subtitle`, `band_title`, `band_line`,
  `no_reason`) with `ui::words(Lang) -> &'static Words`; `SWITCH_LABELS`, `SwitchPart`, `switch_origins` and
  `switch_hit` removed (the pill replaces them; `ui::SWITCH: [(&str, Lang); 2] = [("English", Lang::En), ("日本語", Lang::Ja)]`).

- [ ] **Step 1: Tests**: `the_view_of_a_call` (Japanese: icon Lock, subtitle, `("06:05:00", "2026-11-15")`, reason
  Given, chips Claude `Claude Code (pid 42) · Windows Terminal`, Repo `me/r · ?`, Folder `~\src\r`, code label コマンド,
  code `op vault list`, notes = the lock-skipped text, band of §5 with its marks, footer = `JA.footer`);
  `herdr_and_a_person_in_chips` (Person chip `Human (terminal)`, Herdr `herdr · lab · ?`);
  `the_status_ui_is_unchanged` (`json()` of today's test facts equals today's shape); `table_matches_the_launcher`
  unchanged.
- [ ] **Step 2: Run** → fails.
- [ ] **Step 3: Implement** in `build()` (the view next to the lines).
- [ ] **Step 4: Run** → pass.
- [ ] **Step 5: Commit** `feat(win): the view of an op call's window`.

### Task 4: The relay's `View` and the line about how long an answer lasts

**Files:**
- Modify: `crates/op-reason-win/src/relay_ui.rs`, `crates/op-reason-win/src/win/relay/conn.rs` (the structured root)

**Interfaces:**
- Produces: `relay_ui::Root { name: String, pid: u32, cmd: String }`, `Facts.root: Option<Root>`; `relay_ui::build`
  leaves `lines` empty and fills `view` (§4.3, §5); `Texts.footer` = the view's footer (Asking) or status (Ask).

- [ ] **Step 1: Tests** (rewritten against the view): `the_heads_and_buttons_of_both_phases` (as today);
  `the_view_of_a_call_that_asks` (English: icon Key, subtitle `op-reason's relay · per-call mode`, chips, code label
  `Root command`, code `git push origin fix/login` for the root `bash -c -l "git push origin fix/login"`, code note
  `bash (pid 31337) · bash -c -l "git push origin fix/login"`, facts Purpose `logging in as git`, Detail
  `Destination git@github.com · host key SHA256:h · Key GitHub · SHA256:k`, Mono `Command: ssh.exe …`, countdown
  `{0} s left to answer`, status `Allowing covers this git push until it ends (at most 60 minutes)`, footer empty);
  Japanese status `許可は、この git push が終わるまで効きます (長くても 60 分)`;
  `a_shell_line_says_this_call` (`cd a && git push` → `…covers this call until it ends…`);
  `no_reason_given` (Reason::Missing with the caution text); `while_1password_asks` (band line = today's tray line,
  footer = the deny line, three band titles for per-call, slow, show-only, no countdown);
  `a_request_without_a_root_shows_the_ssh_command` (forwarded: code label `Command`, code = the ssh command, no Mono
  fact, status `This answer covers this signature only`); `waiting_calls` (status and Asking footer end with the count);
  `wsl_calls_name_herdr_and_the_distribution` (chips `WSL · Ubuntu`, `herdr · デプロイ · w1:p2`); the toast tests unchanged.
- [ ] **Step 2: Run** → fails.
- [ ] **Step 3: Implement**, and in `conn.rs` fill `Root` where `root_line` was built (both the Windows chain and
  `from_wsl`).
- [ ] **Step 4: Run** → pass; clippy for the Windows target clean.
- [ ] **Step 5: Commit** `feat(win): the relay's view; how long an answer lasts, tied to the root command`.

### Task 5: The window draws the view

**Files:**
- Modify: `crates/op-reason-win/src/win/window.rs`, `crates/op-reason-win/src/win/relay/ask.rs` (the deadline)

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces: `Window::set_deadline(&self, deadline: Instant, total: Duration)` (the countdown; a 250 ms timer redraws
  it); the layout constants `PAD_LEFT = 14.0`, `PAD_RIGHT = 14.0`, `PAD_TOP = 12.0`, `PAD_BOTTOM = 11.0`,
  `BUTTON_H = 30.0`, `BUTTON_RADIUS = 6.0`, `BUTTON_MIN_W = 96.0`, `PILL_H = 24.0`, `PILL_PAD = 9.0`, `SKETCH = (64.0, 46.0)`.

- [ ] **Step 1:** Lay out the view (§4.2, §4.3) into positioned parts (text layouts with their color roles and the
  ranges colored at draw time with `SetDrawingEffect`, shapes, links, the pill, the buttons, the countdown's bar and
  row); draw them with the theme's palette; hit the pill with `look::pill_hit`, links and buttons as today. The
  footer's row (or the countdown's) ends at the bottom with the pill; the buttons' row ends 11 DIPs above the bottom.
- [ ] **Step 2:** The theme: read at `create`, `win::theme::frame` after `CreateWindowExW`, `WM_SETTINGCHANGE` →
  read again, frame again, redraw.
- [ ] **Step 3:** `ask.rs`: after `Shown::open` in `ask()`, `shown.window.set_deadline(deadline, answer wait)`.
- [ ] **Step 4: Run** clippy for the Windows target and on Linux, `cargo test` on Linux → clean.
- [ ] **Step 5: Commit** `feat(win): draw the windows in the new look`.

### Task 6: The toasts in the same colors and fonts

**Files:**
- Modify: `crates/op-reason-win/src/win/pass_window.rs`, `crates/op-reason-win/src/win/window.rs` (a shared monospace
  format: `window::mono_family(&IDWriteFactory) -> &'static str`, `window::format_in(dw, family, weight, size, locale)`)

- [ ] **Step 1:** The paint from the palette of the theme read when a card comes to an empty stack (§4.12); line 2 in
  the monospace font.
- [ ] **Step 2: Run** clippy (both targets) → clean.
- [ ] **Step 3: Commit** `feat(win): the toasts in the windows' colors and fonts`.

### Task 7: Screenshots in both themes, and the first look on CI

**Files:**
- Modify: `.github/scripts/windows-exe-shots.ps1`

- [ ] **Step 1:** A helper that sets `HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize`
  `AppsUseLightTheme` (DWORD) and one that puts the old value back at the end (also on failure). Today's scenes in the
  dark theme; `09-exe-refs-links-ja`, `13-relay-claude-ja`, `14-relay-person-en`, `15-relay-show-only-en` and
  `16-relay-toasts-ja` again in the light theme, named `…-light`.
- [ ] **Step 2:** Push the branch, wait for `Windows exe`, download `windows-exe-screenshots`, look at every scene;
  fix what is off (overlaps, cut text, contrast) and repeat.
- [ ] **Step 3: Commit** `test(win): screenshots of the windows in both themes`.

### Task 8: Documents and the README images

**Files:**
- Modify: `README.md`, `README.ja.md`, `docs/images/make_readme_svgs.py` (and the SVGs it writes),
  `docs/html/relay-screens.html`, `docs/architecture/README.md`, `docs/manual-verification.md`,
  `docs/manual-verification.ja.md`, `CHANGELOG.md`

- [ ] **Step 1:** README texts (spec §9) and alt texts; the line of §4.8 in "Two modes".
- [ ] **Step 2:** The animations: the window of an `op` call, the language scene, the relay's window and the toasts in
  the dark palette and the new layout; the copied texts and constants follow the Rust sources;
  `python3 docs/images/make_readme_svgs.py` then `--check` → passes.
- [ ] **Step 3:** The HTML page's window and toasts in the same look.
- [ ] **Step 4:** Architecture, manual checks (both themes, a theme change while a window is open, 150%), CHANGELOG.
- [ ] **Step 5: Commit** `docs: the new look of the windows`.

### Task 9: Review and the pull request

- [ ] **Step 1:** A fresh reviewer reads the branch against the spec; fix what holds.
- [ ] **Step 2:** All lints and tests of the Global Constraints; CI green; screenshots looked at.
- [ ] **Step 3:** `git log --format=%B origin/main..` has no ticket ID and no other repository's reference; push; open
  the pull request with the screenshots' comment and the decisions of §10.
