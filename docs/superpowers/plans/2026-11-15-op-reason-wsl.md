# op-reason PR 1 (WSL side in Rust) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the WSL `op` wrapper (bash) to one Rust executable `op-reason`, with the shared launcher speaking the new
facts-based payload, black-box tests ported from the dotfiles, CI, Nix flake and documents.

**Architecture:** A Cargo workspace: `op-reason-core` (library, builds on Linux and Windows: argument planning, contract
types, log, log viewer, place detection, messages, embedded launcher) and `op-reason` (the WSL binary: `argv[0]`
dispatch, `/proc`, flock, signals, starting the launcher). The launcher stays PowerShell (Windows PowerShell 5.1) and
builds the window texts from facts.

**Tech Stack:** Rust 2024 (MSRV 1.89), serde/serde_json, chrono (clock), libc; Windows PowerShell 5.1 (tested with pwsh
on Linux); bash/fish black-box tests; Nix (dev shell and package); GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-11-15-op-reason-design.md`

## Global Constraints

- Edition 2024, `rust-version = "1.89"`, workspace lints: `clippy::all` deny in CI (`cargo clippy --all-targets -- -D warnings`).
- Dependencies: `serde` (derive), `serde_json`, `chrono` (`default-features = false`, `features = ["clock", "std"]`),
  `libc` (unix only). Every version published at least 7 days before it is added.
- Environment names exactly: `OPREASON_LANG`, `OPREASON_LOCK_WAIT`, `OPREASON_OP_EXE`, `OPREASON_POWERSHELL`,
  `OPREASON_SCRIPT`, `OPREASON_PAYLOAD`; test hooks `OPREASON_PROC_ROOT`, `OPREASON_PROC_START`, `OPREASON_LOG_NOW`.
- State: `${XDG_STATE_HOME:-~/.local/state}/op-reason/` with `log.jsonl`, `log.1.jsonl` (rotate at 1048576 bytes),
  `log.lock`, `lock`, `current.json`, `run/<id>/status.json`.
- Log records exactly as spec 5.6 (`v: 1`, same field names and order as the bash version).
- Exit codes exactly as spec 5.7. Lock wait default 90 s. Too long: base64 of arguments > 28000, payload > 32000.
- stderr prefix `op-reason: `; launcher prefix `op-reason: launcher: `.
- `launcher/launcher.ps1`: UTF-8 with BOM, Windows PowerShell 5.1 syntax; base64 of its UTF-8 bytes < 30000 characters.
- Public repository: no private repository names or paths, no author real name or e-mail, no references to other
  repositories' issues/PRs/commits in commit messages. Comments in English.
- Tests never run a real `op.exe`, `powershell.exe` or 1Password.

## Review Focus

1. Non-UTF-8 bytes in arguments or in the reason (Linux `argv` is bytes): no panic; `op` receives the original bytes
   (args64 is base64 of the bytes); log and window use lossy text. — test in Task 6 (`wrapper.test.sh`).
2. Reasons and arguments with multi-byte characters and emoji at the clip boundary: cut by characters, never inside a
   UTF-8 sequence. — unit test in Task 2.
3. Current directory removed or unreadable: the call still runs; place falls back to `$PWD` or `?`. — test in Task 4
   (unit) and Task 6 (black box).
4. Broken state files (`current.json`, `status.json`, `log.jsonl` with junk or a partial line): no panic;
   `launcher-failed`, holder "(unknown)", junk lines skipped. — tests in Tasks 3, 6 and 7.
5. stdout closed early (`op-reason --skill | head -1`, `op … | head -c1`): no panic message; the wrapper exits with
   `op`'s exit code; usage/skill/log output ignores EPIPE. — test in Tasks 6 and 7.

---

### Task 1: Workspace, helpers, development shell

**Files:**
- Create: `Cargo.toml` (workspace, `resolver = "3"`, shared `[workspace.package]` and `[workspace.dependencies]`),
  `crates/op-reason-core/{Cargo.toml,src/lib.rs,src/base64.rs,src/json.rs,src/time.rs}`,
  `crates/op-reason/{Cargo.toml,src/main.rs}`, `nix/flake.nix`, `nix/flake.lock`, `.editorconfig`, `.gitignore`,
  `LICENSE-MIT`, `LICENSE-APACHE`, `rust-toolchain.toml` (none: the dev shell pins rustc)

**Interfaces:**
- Produces: `base64::encode(&[u8]) -> String`, `base64::decode(&str) -> Option<Vec<u8>>`;
  `json::to_ascii(&str) -> String` (non-ASCII → `\uXXXX`, surrogate pairs above U+FFFF);
  `time::Now { epoch: i64, iso: String /* %Y-%m-%dT%H:%M:%S%:z */, id_stamp: String /* %Y%m%dT%H%M%S */, hms: String,
  ymd_hms: String }`, `time::now() -> Now`, `time::local_parts(epoch: i64) -> (String /* %Y-%m-%d */, String /* %H:%M:%S */, String /* %m/%d */)`.

- [ ] **Step 1:** Write unit tests: base64 round trip of all byte values and of `""`, `"f"`, `"fo"`, `"foo"` (RFC 4648
  vectors); `to_ascii("{\"a\":\"日本😀\"}") == "{\"a\":\"\\u65e5\\u672c\\ud83d\\ude00\"}"`; `time::local_parts` with
  `TZ=UTC0` for epoch 1800000000 gives `("2027-01-15", "08:00:00", "01/15")`.
- [ ] **Step 2:** Run `nix develop path:./nix --command cargo test -p op-reason-core` → fails (no code).
- [ ] **Step 3:** Implement the helpers; `nix/flake.nix` dev shell with `rustc cargo clippy rustfmt powershell fish jq
  shellcheck shfmt util-linux git coreutils`; pin `nix/flake.lock` with `flake-lock-age update nix`.
- [ ] **Step 4:** Tests pass; `cargo clippy --all-targets -- -D warnings` clean.
- [ ] **Step 5:** Commit `build: set up the Cargo workspace and the development shell`.

### Task 2: Argument planning and the call table

**Files:**
- Create: `crates/op-reason-core/src/args.rs`, `tests/cases/calls.json`, `crates/op-reason-core/tests/calls.rs`

**Interfaces:**
- Produces:
  - `pub struct Parsed { pub reason: Option<String> /* normalized */, pub op_args: Vec<OsString> }`,
    `pub fn parse(args: &[OsString]) -> Parsed`
  - `pub fn normalize(&str) -> String`, `pub fn clip(&str, n: usize, ellipsis: &str) -> String` (characters)
  - `pub fn is_help(op_args: &[String]) -> bool`, `pub fn mask(op_args: &[String]) -> Vec<String>`,
    `pub fn display(masked: &[String]) -> String`
  - `pub enum Name { OpReason, Other }`, `pub fn name_of(argv0: &OsStr) -> (Name | LogViewer)` — represented as
    `pub enum Invocation { Wrapper(Name), LogViewer }`
  - `pub enum Plan { Usage, Version, Skill, Completion { shell: String, command: String }, Log(Vec<OsString>),
    Refuse { why: &'static str, exit: i32 }, Run { mode: Mode, kind: Kind } }` with `Mode { Desktop, Help, Service }`,
    `Kind { Desktop, Help, ServiceAccount, Connect }` (`as_str()` gives the log words), and
    `pub fn plan(name: Name, args: &[OsString], env: &dyn Fn(&str) -> Option<String>) -> (Parsed, Plan)`.
- `calls.json` entries: `{ "name", "invokedAs": "op"|"op-reason", "argv": [...], "env": {...}, "expect": { "action":
  "usage"|"version"|"skill"|"completion"|"log"|"refuse"|"run", "why", "exit", "mode", "kind", "opArgs", "masked",
  "command", "reason" } }`.

- [ ] **Step 1:** Write `calls.json` with at least: no args; `--reason` only; `--reason X a b` / `a --reason X b` /
  `a b --reason=X` → op args `a b`; `a -- b --reason X` → refuse no-reason 2; empty and blank reasons → refuse 2; help
  forms `--version`, `--help`, `item get --help`, `help item` without reason → run help/help; `op-reason --help`,
  `-h`, `--version`, `-V` → usage/version; `op --help` → run help; `--skill`; `--completion fish`, `--completion fish
  op-reason`, `--completion zsh` → refuse 2 (why `unsupported-shell`); `--log --all`; service account and Connect
  envs (Connect with only one variable → desktop); masking case `item create password=hunter2 --session sess1
  --secret-key=A3-XYZ op://v/i/f?attribute=otp` → masked `item create password=*** --session *** --secret-key=***
  op://v/i/f?attribute=otp`; `run -- curl -s -H "Authorization: Bearer tok" https://e/?k=v` → `run -- curl -s -H ***
  ***`; display of `item get "a b"` → `op item get "a b"`; reason normalization `"  直す\tため\nに\x01読む  "` →
  `"直す ため に 読む"`.
- [ ] **Step 2:** Write `tests/calls.rs` that loads `../../tests/cases/calls.json` and asserts every field present in
  `expect`; unit tests for `clip` at multi-byte and emoji boundaries (`clip("あ😀い", 2, "…") == "あ😀…"`).
- [ ] **Step 3:** Run → fails. **Step 4:** Implement `args.rs` (bash `op.sh` lines 172–282 are the reference). Run → passes.
- [ ] **Step 5:** Commit `feat(core): plan wrapper invocations from a shared call table`.

### Task 3: Contract types, log, results, messages

**Files:**
- Create: `crates/op-reason-core/src/{contract.rs,log.rs,result.rs,messages.rs,lang.rs}`

**Interfaces:**
- Produces:
  - `contract`: `Payload { v, mode, op, args64, status, lang, lock, lock_wait (#[serde(rename = "lockWait")]), stderr,
    facts: Option<Facts>, notify: Option<Notify> }`, `Facts { time, caller: FactsCaller { r#type, pid, session,
    detail }, herdr: Option<Herdr>, place: Place { repo, branch, cwd }, reason, command, notes: Vec<Note> }`,
    `Note { kind, holder_reason (#[serde(rename = "holderReason")]) }`, `Status { started, exit, error, window,
    toast, others, signout, lock, holder, stderr_match }` (all `Option`/default, unknown fields ignored); constants
    `ENV_PAYLOAD = "OPREASON_PAYLOAD"`, `ENV_SCRIPT = "OPREASON_SCRIPT"`, `MUTEX_NAME = r"Local\op-reason"`,
    `EXIT_USAGE = 2`, `EXIT_LOCK = 75`, `EXIT_MISSING = 127`.
  - `log`: `Caller { r#type, claude_pid, herdr: Option<Herdr>, cwd, repo, branch }` (log shape),
    `Record::{Start, End, Refused, Shared}` builders that serialize in the bash field order,
    `pub fn append(state_dir: &Path, line: &str)` (lock `log.lock` up to 5 s, rotate at 1 MiB, never fails the call),
    `pub fn new_id(now: &time::Now) -> String`.
  - `result`: `pub fn classify(status: Option<&Status>, op_exit: i32, mode: Mode, stderr_tail: &[u8]) -> &'static str`.
  - `lang`: `pub enum Lang { En, Ja }`, `pub fn resolve(env: &dyn Fn(&str) -> Option<String>) -> Lang`.
  - `messages`: `pub fn usage_full(Lang, version) -> String`, `pub fn usage_short(Lang) -> String`, and one function per
    stderr message of spec 6.1 (no-reason, empty reason, too long, lock waiting, lock timeout, missing tools), plus
    `notify_title_body(Lang, when, cleared) -> (String, String)` for herdr.

- [ ] **Step 1:** Tests: record JSON equals the bash shape for each event (compare against literal strings);
  `append` rotates when the file is ≥ 1048576 bytes and keeps the new line in `log.jsonl`; `classify` covers approved
  / ok / denied (`[ERROR] 2026/11/08 00:00:00 authorization prompt dismissed, please try again`) / timeout
  (`[ERROR] … authorization timeout`) / failed (`curl: (28) Connection timed out after 1000 ms`) / launcher-failed
  (no status, `started:false`, status that is not an object); `Status` parses junk-free JSON with unknown fields and
  rejects arrays; `resolve` with `OPREASON_LANG=ja`, `LANG=ja_JP.UTF-8`, `LC_ALL=C` + `LANG=ja_JP.UTF-8` (→ en),
  unset (→ en).
- [ ] **Step 2:** Run → fails. **Step 3:** Implement. **Step 4:** Run → passes.
- [ ] **Step 5:** Commit `feat(core): add the launcher contract, log records and messages`.

### Task 4: Place detection without git

**Files:**
- Create: `crates/op-reason-core/src/place.rs`

**Interfaces:**
- Produces: `pub struct PlaceInfo { pub repo: Option<String>, pub branch: Option<String> }`,
  `pub fn detect(cwd: &Path) -> PlaceInfo`, `pub fn repo_from_url(url: &str) -> String`,
  `pub fn tilde(path: &str, home: Option<&str>) -> String`, `pub fn display(info: &PlaceInfo, cwd_display: &str) -> String`.

- [ ] **Step 1:** Tests with temporary directories: `ref: refs/heads/feat/x` → branch `feat/x`; detached hash →
  `(detached 0123456)`; `.git` file with `gitdir:` and `commondir` (worktree) → branch from the worktree `HEAD`, origin
  from the common `config`; `[remote "origin"]` with `url = git@github.com:o/r.git` → `o/r`;
  `https://github.com/o/r/` → `o/r`; no origin → directory name; outside a repository → both `None`; a missing
  directory → both `None`.
- [ ] **Step 2–4:** Fail, implement, pass.
- [ ] **Step 5:** Commit `feat(core): read the repository and branch from git files`.

### Task 5: Launcher with facts and languages

**Files:**
- Modify (port): `launcher/launcher.ps1` (from the dotfiles `launcher.ps1`)
- Create: `crates/op-reason-core/src/launcher.rs`, `tests/launcher.test.sh` (port of `pjp-op-launcher.test.sh`)

**Interfaces:**
- Produces: `launcher::script_b64() -> String` (BOM removed), `launcher::encoded_command() -> String` (UTF-16LE base64 of
  the bootstrap of spec 5.1), `launcher::SOURCE: &[u8]`.
- Launcher functions: `Get-Texts $lang` (message table), `Build-Ui $facts $lang` → `@{ title; lines; footer; others;
  toast }`, `Build-NotifyUi $notify $lang`.

- [ ] **Step 1:** Port the launcher test: same checks as the dotfiles (tricky arguments, binary stdin, stderr only from
  `op`, exit code, payload variables not inherited, bootstrap path, whoami/signout, help, service, missing `op`, notify)
  with the new names, plus: `ui.lines` labels for `lang: ja` are `時刻,呼び手,場所,理由,コマンド` (and `herdr` when
  present), for `en` `Time,Caller,Place,Reason,Command`; the caller line is `Claude Code (pid 999)` / `人 (端末)` /
  `Human (terminal)`; footer contains `この窓が出ていない` / `without this window`; toast has 3 texts; notes render as
  `注意`/`Note` lines.
- [ ] **Step 2:** Rust unit test: `script_b64().len() < 30000`; `encoded_command()` decodes to the bootstrap text.
- [ ] **Step 3:** Run (`bash tests/launcher.test.sh` in the dev shell) → fails. **Step 4:** Implement → passes.
- [ ] **Step 5:** Commit `feat(launcher): build window texts from facts in English and Japanese`.

### Task 6: The WSL wrapper

**Files:**
- Create: `crates/op-reason/src/{wrapper.rs,procfs.rs,lock.rs,signals.rs,launch.rs}`, `tests/wrapper.test.sh` (port of
  `pjp-op.test.sh`), `tests/e2e.test.sh` (port of `pjp-op-e2e.test.sh`), `tests/lib.sh` (shared `ok`/`ng`/`fake`)

**Interfaces:**
- Consumes: Tasks 2–5.
- Produces: `wrapper::run(name: Name, args: Vec<OsString>) -> i32`; `procfs::find_claude(root, start) -> Option<u32>`,
  `procfs::op_upstream() -> bool`, `procfs::feeds(holder_pid) -> bool`; `lock::acquire(...) -> LockOutcome { Held,
  Skipped { holder_reason }, Timeout { holder } }`; `signals::install(ctx: Arc<Mutex<SignalCtx>>)`;
  `launch::run_with_timeout(cmd, Duration) -> Option<ExitStatus>`, `launch::wslenv_plus(items) -> String`.

- [ ] **Step 1:** Port `pjp-op.test.sh` to `tests/wrapper.test.sh` against `$OPREASON_BIN` with `op` symlinks: every
  check of the original with the new names; payload checks read `facts` (`facts.reason`, `facts.command`,
  `facts.caller`, `facts.herdr`, `facts.notes`) instead of `ui.lines`; `XDG_RUNTIME_DIR` and stderr-copy checks
  replaced by "no file left under the state directory except logs, locks"; plus Review Focus 1, 3, 4, 5:
  non-UTF-8 argument reaches the fake `op.exe` byte for byte; a removed current directory still runs; a junk
  `current.json` gives "(unknown)" in the lock timeout message; `op … | head -c1` exits without a panic message.
- [ ] **Step 2:** Port `pjp-op-e2e.test.sh` (fake `powershell.exe` runs `pwsh` with the real launcher through
  `OPREASON_SCRIPT` and the bootstrap).
- [ ] **Step 3:** Run both → fail. **Step 4:** Implement the flow of spec 6.1 (bash `op.sh` lines 284–860 are the
  reference). Run both → pass, several times in a row for the pipe cases.
- [ ] **Step 5:** Commit `feat: port the WSL wrapper to Rust`.

### Task 7: Log viewer

**Files:**
- Create: `crates/op-reason-core/src/logview.rs`, `tests/log.test.sh` (port of `pjp-op-log.test.sh`)

**Interfaces:**
- Produces: `logview::run(args: &[OsString], state_dir: &Path, now: i64, lang: Lang, out: &mut dyn Write, err: &mut dyn
  Write) -> i32`; `display_width(&str) -> usize`.

- [ ] **Step 1:** Port the log test with `OPREASON_LANG=ja` (all original assertions) and add English assertions
  (`approved`, `refused (no reason)`, `running or interrupted`); junk and partial lines skipped; `--json` length 8.
- [ ] **Step 2–4:** Fail, implement, pass.
- [ ] **Step 5:** Commit `feat: add the log viewer`.

### Task 8: Skill file and fish completion

**Files:**
- Create: `skill/SKILL.md`, `completions/op.fish` (template with `__OPREASON_CMD__`), `tests/completion.test.sh` (port)

**Interfaces:**
- Produces: `--skill` prints `skill/SKILL.md` (`include_str!`), `--completion fish [cmd]` prints the template with the
  command substituted.

- [ ] **Step 1:** Port the completion test against `op-reason --completion fish` output (for `op`) and add one check
  for `op-reason` as the command; check `op-reason --skill` starts with `---\nname: op-reason\n`.
- [ ] **Step 2–4:** Fail, implement, pass.
- [ ] **Step 5:** Commit `feat: print the agent skill and the fish completion`.

### Task 9: Package, CI, release

**Files:**
- Create: `flake.nix`, `flake.lock`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `tests/run-all.sh`

- [ ] **Step 1:** `flake.nix`: `packages.default` (buildRustPackage; `bin/op-reason`, `bin/op-reason-log` symlink,
  `share/fish/vendor_completions.d/op-reason.fish`), `packages.op` (adds `bin/op` and `op.fish`); `nix build` works.
- [ ] **Step 2:** `ci.yml`: Linux job in `nix develop path:./nix` (fmt check, clippy, `cargo test`, `tests/run-all.sh`,
  shellcheck on `tests/*.sh`, flake lock age check for `nix/` and the root); Windows job (`cargo test -p
  op-reason-core`). Actions pinned by commit SHA.
- [ ] **Step 3:** `release.yml` on `v*` tags: musl builds on x86_64 and arm64 runners, `sha256sum` files, `gh release
  create --draft` with the assets.
- [ ] **Step 4:** Push the branch and confirm CI passes.
- [ ] **Step 5:** Commit `ci: test on Linux and Windows, build releases on tags`.

### Task 10: Documents

**Files:**
- Create: `README.md` (rewrite), `README.ja.md`, `docs/README.md` (with the map diagram), `docs/plantuml/map.puml` and
  its SVG, `docs/manual-verification.md`, `docs/manual-verification.ja.md`

- [ ] **Step 1:** README (English): Japanese link first line, unofficial notice, what and why, how it works, install
  (release binary + symlinks, Nix), usage, options, environment variables, logs, exit codes, agents (`--skill`), fish,
  caveats (`signout --all` scope, lock, pipes), license. README.ja.md: the same in Japanese.
- [ ] **Step 2:** Manual verification (both languages): approval confined to one call, window texts in both languages,
  denial → hook-visible `denied`, timeout, lock between two panes, pipe `op … | op …`, shared approval watchdog, plain
  `signout` scope (the open question), log viewer.
- [ ] **Step 3:** `docs/README.md` map (PlantUML) linking README, spec, manual verification.
- [ ] **Step 4:** Commit `docs: add the README, the Japanese README and the manual checks`.
