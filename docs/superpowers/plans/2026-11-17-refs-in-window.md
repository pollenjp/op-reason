# Plan: the references a call reads, in the window

Design: `docs/superpowers/specs/2026-11-17-refs-in-window-design.md`. Tests first where the code is portable; the
Windows-only code is type-checked on Linux (`cargo clippy --target x86_64-pc-windows-msvc`) and tested by the Windows
CI jobs.

## 1. Core: collecting references (`crates/op-reason-core/src/refs.rs`)

- [ ] `tests/cases/refs.json`: calls (arguments, files, stdin) with the expected `list`, `from` and `account`; ID
  references and non-ID references. `crates/op-reason-core/tests/refs.rs` reads it (fails first).
- [ ] Unit tests, then the code: `scan_template` (§2.1), `scan_env_file` (§2.2), `shown` (§2.3), `reads_stdin`,
  `collect`, `link_ids`, `Collected::facts` (8 of N, clips) and `Collected::log` (1000, clips).
- [ ] `contract::Refs`/`RefSource` and `Facts.refs` (optional, skipped when empty); the round-trip test covers it.
- [ ] `log::Start.refs` (skipped when none); `logview` adds the count to the command when a source other than the
  arguments gave references; usage says `--json` has the references.

## 2. WSL executable (`crates/op-reason/src/wrapper.rs`)

- [ ] Desktop calls: `op inject` without `-i` and stdin not a terminal → read stdin to the end before the lock (the
  existing drained copy reaches the runner); collect references (files relative to the current directory); put
  `facts.refs` in the payload and `refs` in `start`.
- [ ] `tests/wrapper.test.sh`: the fake runner sees `facts.refs` and the same stdin bytes; `-i` leaves stdin alone;
  env files; the log and `op-reason-log`.

## 3. `op-reason.exe`

- [ ] `src/account.rs` (portable): read the whoami JSON (ID and host only), merge into `account.json` (5, most recent
  first, temporary file and rename), choose by `--account`, build the `view-item` URL. Unit tests.
- [ ] `src/ui.rs`: the table keys `refs`, `refsArgs`, `refsStdin`, `refsSep`, `refsMore`, `refsHint`; `Texts.refs`
  (head, items with URL, more, hint) after the command line; `json()` writes `ui.refs` with `linked`. Unit tests;
  `table_matches_the_launcher` covers the new keys.
- [ ] `src/win/window.rs`: indented paragraphs, link paragraphs (blue, underlined), DirectWrite hit test, the hand
  cursor, press and release on the same link, `open_link` (new `src/win/shell.rs`: `AllowSetForegroundWindow`, COM,
  `ShellExecuteW` on a thread; the window waits up to two seconds for it when it closes). Cargo features.
- [ ] `src/win/opexe.rs`: `whoami --format json` with stdout kept (bounded); `close_session` returns the account;
  `start` can feed op.exe's stdin from bytes.
- [ ] `src/win/app.rs`, `run.rs`: the entry face collects references (stdin read when not a console, then piped to
  op.exe); both faces choose the account and build the texts with it; the account is written after whoami.
- [ ] Fake op (`fake-op`): `FAKE_WHOAMI_OUT` printed by whoami; an `onepassword:` first argument is only recorded.
- [ ] `tests/exe.rs`: `ui.refs` on both faces, a template on stdin, `account.json` after whoami, the CI-only click on a
  link with a temporary `onepassword:` handler.

## 4. Launcher and PowerShell entry

- [ ] `launcher/launcher.ps1`: the new texts; `Build-Ui` with the account (read `account.json`, choose, check, URL);
  the window's block with not focusable `Hyperlink`s, the `WM_MOUSEACTIVATE` hook, `Process.Start` on `Click`;
  `Close-Session` runs `whoami --format json` and writes `account.json`.
- [ ] `windows/op-reason.ps1`: references from arguments and files (never stdin) into `facts.refs`, `start.refs`, and the
  plan-only output.
- [ ] `tests/windows/fake-op.cs` and the bash fake ops: `FAKE_WHOAMI_OUT`, the `onepassword:` record.
- [ ] `tests/launcher.test.sh`, `tests/e2e.test.sh`, `tests/windows/launcher.test.ps1`, `entry.test.ps1`,
  `entry-plan.test.ps1` (the shared table); the whoami sequences become `whoami --format json`.

## 5. Screenshots, documents, checks

- [ ] `.github/scripts/windows-exe-shots.ps1`: scenes with linked and plain references, "and N more", the hint;
  `tests/windows/launcher.test.ps1`: the launcher's window with the block.
- [ ] READMEs, `docs/architecture/README.md`, `docs/manual-verification*.md` (R1–R6), `skill/SKILL.md`,
  `CHANGELOG.md`.
- [ ] `cargo fmt`, `cargo clippy` and `cargo test` in both workspaces, the Windows type-check, `bash tests/run-all.sh`,
  shellcheck and shfmt, `make_readme_svgs.py --check`; push; the pull request; the screenshots on it.
