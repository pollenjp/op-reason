#!/usr/bin/env python3
"""Makes the animations of the READMEs (docs/images/readme-*.svg), in English and Japanese.

- One SVG per scene and language. They move with CSS @keyframes only: GitHub shows README images without running
  scripts.
- Each SVG has its own background, so that it reads the same on GitHub's light and dark themes. It uses the
  viewer's fonts (an image cannot load web fonts).
- With reduced motion (prefers-reduced-motion: reduce), each SVG holds still on the frame that carries its point
  (scene 1: op-reason's window and the approval prompt), so that nothing is lost without the motion. Viewers
  that run no CSS animations show the same frame.
- op-reason's window is laid out as crates/op-reason-win/src/win/window.rs draws it, with the texts of
  crates/op-reason-win/src/ui.rs (and its language switch). The refusal and the log table are op-reason's output
  (the texts of crates/op-reason-core/src/messages.rs and logview.rs), the saved choice of a language is the file of
  crates/op-reason-core/src/settings/file.rs. The 1Password approval prompt is a plain box: it copies neither the look
  nor the words of 1Password's screen.
- The SSH relay's scene: its window with the texts of crates/op-reason-win/src/relay_ui.rs, the toasts of signatures
  that passed without a window as crates/op-reason-win/src/win/pass_window.rs draws them, moving by the rules and
  times of crates/op-reason-win/src/pass_toast.rs (simulated here), and a Windows notification. The HTML page
  docs/html/relay-screens.html plays the same; --check also looks for the copied texts and times in it.
- Names are examples only (repository, directory, process ids).

To change a text or the layout, change this script and run it again; do not edit the SVGs. Standard library only.

    python3 docs/images/make_readme_svgs.py          # write the SVGs
    python3 docs/images/make_readme_svgs.py --check  # compare them instead (CI)

`--check` also fails when a text or a size copied from the Rust sources is no longer found there.
"""

from __future__ import annotations

import argparse
import itertools
import sys
import unicodedata
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
LANGS = ("en", "ja")

W = 880  # every SVG is this wide, so that the README shows them at one scale

# ---- fonts and colors ----

UI = (
    '"Segoe UI", "Yu Gothic UI", Meiryo, "Hiragino Sans", "Hiragino Kaku Gothic ProN", "Noto Sans CJK JP", '
    '"Noto Sans JP", system-ui, sans-serif'
)
MONO = (
    '"Cascadia Mono", Consolas, Menlo, "DejaVu Sans Mono", "Noto Sans Mono", "Noto Sans Mono CJK JP", '
    '"BIZ UDGothic", ui-monospace, monospace'
)
# Monospace fonts advance 0.55 em (Consolas) to 0.602 em (DejaVu Sans Mono, Menlo). Terminal text is laid out on a
# grid of Cascadia Mono's 0.586 em per column (two columns for a wide character, as op-reason-log counts them), and
# textLength fits each run to its columns, so that the columns line up whatever font the viewer has.
ADVANCE = 0.586

INK = "#1a1a1a"
MUTED = "#4b5563"
LINE = "#c9ced6"
TERM_BG = "#0c0c0c"
TERM_BAR = "#202020"
TERM_FG = "#cccccc"
TERM_DIM = "#8b8b8b"
TERM_PATH = "#3b8eea"
TOOL_WAIT = "#8b8b8b"
TOOL_DONE = "#3fb950"
BAND = "#fbbf24"
RED = "#b91c1c"
GREEN = "#15803d"
APPROVE = "#2563eb"

# op-reason's window: the layout (DIPs) and colors of crates/op-reason-win/src/win/window.rs. What Windows draws
# around it (the frame, the caption) is taken from the screenshots of the windows-exe workflow.
WIN_OUTER = 520.0  # OUTER_WIDTH
WIN_W = WIN_OUTER - 2 * 7  # the frame as seen: Windows draws nothing in the outer 7 DIPs on both sides
WIN_RIGHT = 16 + 8  # SCREEN_MARGIN, then the invisible part of the frame on the right
WIN_CAPTION = 30.0  # the caption of the tool window, below the top border line
PAD_LEFT, PAD_TOP, PAD_RIGHT, PAD_BOTTOM, LINE_GAP = 14.0, 10.0, 14.0, 12.0, 2.0
TEXT_W = WIN_W - 2 - PAD_LEFT - PAD_RIGHT  # 476: the client area (the frame less its border lines) less the paddings
TITLE_SIZE, BODY_SIZE = 16.0, 12.0
SWITCH_SIZE = 11.0  # the language switch, at the right end of the footer's last line
SEGOE_ASCENT = 2210 / 2048  # Segoe UI: DirectWrite puts the baseline this far (em) below the top of a line,
SEGOE_LINE = (2210 + 514) / 2048  # and a line is this tall
WIN_BG = "#ffffe0"  # LIGHT_YELLOW
WIN_INK = "#000000"  # BLACK
WIN_FOOTER = "#b22222"  # FIREBRICK
WIN_LINK = "#0066cc"  # LINK_BLUE: the language not shown, the one a click switches to
WIN_BAR = "#808080"  # GRAY: the bar between the languages
WIN_CAPTION_BG = "#f3f3f3"
WIN_CAPTION_INK = "#9a9a9a"  # an inactive caption: the window never takes the focus
WIN_CLOSE = "#dadada"
WIN_BORDER = "#8796a1"

STEP = "step-end"
MOVE = "cubic-bezier(.4,.1,.2,1)"  # the mouse
HIDDEN, SHOWN = "opacity:0", "opacity:1"

# ---- texts ----

# Copies of op-reason's texts. --check looks for each of them in the Rust sources (SOURCES below).
WINDOW = {  # crates/op-reason-win/src/ui.rs
    "en": {
        "title": "1Password CLI approval request",
        "time": "Time",
        "caller": "Caller",
        "place": "Place",
        "reason": "Reason",
        "command": "Command",
        "footer": "Deny any 1Password approval prompt that comes without this window",
    },
    "ja": {
        "title": "1Password CLI の承認の依頼",
        "time": "時刻",
        "caller": "呼び手",
        "place": "場所",
        "reason": "理由",
        "command": "コマンド",
        "footer": "この窓が出ていない 1Password の承認は拒否してください",
    },
}
SWITCH = (("English", "en"), (" | ", None), ("日本語", "ja"))  # ui.rs: SWITCH_LABELS
# The footer as DirectWrite wraps it: window.rs lays it out narrower by the switch (78.7 DIPs in Segoe UI, with Yu
# Gothic UI for 日本語) and SWITCH_GAP, so the English one takes two lines (as in the screenshots of the windows-exe
# workflow). The switch sits on the baseline of the last line.
FOOTER_LINES = {
    "en": ("Deny any 1Password approval prompt that comes without this", "window"),
    "ja": ("この窓が出ていない 1Password の承認は拒否してください",),
}
JA_WIDTH = 3 * SWITCH_SIZE  # 日本語: three full-width characters, the end of the switch
SETTINGS = ("%LOCALAPPDATA%\\op-reason\\settings.json", '{"v":1,"lang":"ja"}')  # crates/op-reason-core/src/settings/file.rs
PREFIX = "op-reason: "  # crates/op-reason-core/src/messages.rs
USAGE_SHORT = {  # messages.rs: usage_short, printed with a refusal
    "en": (
        'Usage: op --reason "<why you call op>" <op arguments...>',
        "  op is op-reason, a wrapper for the 1Password CLI: every call needs a reason. The reason appears in the"
        " window",
        "  next to the 1Password approval prompt and in the log (op-reason-log). Write what for and what you read or"
        " write,",
        "  in one sentence.",
        '  Example: op --reason "read the Proxmox API token to create the staging VM" read'
        ' "op://Private/proxmox/token"',
        "  Only --version and help (--help, -h, help) work without a reason. Run op without arguments for the full"
        " usage.",
        "  AI agents: run `op-reason --skill` for the full guide.",
    ),
    "ja": (
        '使い方: op --reason "<なぜ op を呼ぶか>" <op の引数…>',
        "  この op は、呼ぶ理由を必須にした wrapper (op-reason) です。理由は 1Password の承認の画面と一緒に出る窓と、",
        "  記録 (op-reason-log) に残ります。何のために何を読む・書くかを 1 文で書いてください。",
        '  例: op --reason "infra の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token"',
        "  理由なしで通るのは --version と help (--help・-h・help) だけです。詳しい使い方は引数なしの op で出ます。",
        "  AI エージェントは `op-reason --skill` で手引きを読んでください。",
    ),
}
NO_REASON = {  # messages.rs: no_reason
    "en": "There is no reason (--reason). People need it too.",
    "ja": "理由 (--reason) がありません。人が打つときも要ります。",
}
LOG = {  # crates/op-reason-core/src/logview.rs
    "en": {
        "header": ("Time", "Result", "Secs", "Caller", "Reason", "Command"),
        "approved": "approved",
        "denied": "denied",
        "refused": "refused",
        "no-reason": "no reason",
        "human": "Human",
    },
    "ja": {
        "header": ("時刻", "結果", "秒", "呼び手", "理由", "コマンド"),
        "approved": "許可",
        "denied": "拒否",
        "refused": "断った",
        "no-reason": "理由なし",
        "human": "人",
    },
}
CLAUDE = "Claude"  # logview.rs: who called, in the table
REASON_CLIP, COMMAND_CLIP = 48, 60  # logview.rs: cells longer than this are cut and get "…"

# logview.rs: display_width counts two columns for these characters (and none for control characters)
WIDE = (
    (0x1100, 0x115F),
    (0x2E80, 0x303E),
    (0x3041, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xA000, 0xA4CF),
    (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF),
    (0xFE30, 0xFE4F),
    (0xFF00, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x1F300, 0x1F64F),
    (0x1F900, 0x1F9FF),
    (0x20000, 0x3FFFD),
)


# The SSH relay (crates/op-reason-win/src/relay_ui.rs): its window and the toast of a signature that passed
RELAY = {
    "en": {
        "title": "SSH key: allow this call?",
        "time": "Time",
        "caller": "Caller",
        "reason": "Reason",
        "command": "Command",
        "destination": "Destination",
        "purpose": "For",
        "host_key": "host key {0}",
        "userauth": "logging in as {0}",
        "claude_pid": "Claude Code (pid {0})",
        "footer": "per-call mode · an answer covers this call for {0} minutes",
        "allow": "Allow this call",
        "deny": "Deny",
        "pass_word": "Passed",
        "pass_call": "allowed for this call",
        "pass_terminal": "allowed for this terminal",
        "no_reason": "no reason given",
    },
    "ja": {
        "title": "SSH の鍵: この呼び出しを許可しますか",
        "time": "時刻",
        "caller": "呼び手",
        "reason": "理由",
        "command": "コマンド",
        "destination": "接続先",
        "purpose": "用途",
        "host_key": "ホストの鍵 {0}",
        "userauth": "{0} としてログイン",
        "claude_pid": "Claude Code (pid {0})",
        "footer": "呼び出しごとに決めるモード · 答えはこの呼び出しに {0} 分効きます",
        "allow": "この呼び出しを許可",
        "deny": "拒否",
        "pass_word": "通過",
        "pass_call": "この呼び出しの許可",
        "pass_terminal": "ターミナルの許可",
        "no_reason": "理由なし",
    },
}
NOTIFY = {  # crates/op-reason-win/src/ui.rs: the second line of a window's notification, and the short name of a person
    "en": {"toast_reason": "Reason: {0}", "human": "Human"},
    "ja": {"toast_reason": "理由: {0}", "human": "人"},
}
# The toasts (crates/op-reason-win/src/pass_toast.rs): milliseconds and DIPs
SHOW_MS, FADE_MS, ENTER_MS, APPEAR_MS, EVICT_MS, BUMP_MS = 4000, 800, 240, 220, 140, 420
MAX_CARDS = 3
EDGE, GAP, CARD_W, CARD_H, DROP = 12.0, 8.0, 300.0, 70.0, 12.0
EVICT_LINGER_MS = 40  # pass_toast.rs: a card pushed out is dropped this long after it has gone transparent
OPACITY = 0.9  # a card shown (the text behind shows through faintly); the ring of its glow is drawn at full strength
MORE_GAP, MORE_H = 4.0, 16.0  # the tag +N: this far under the third place, this high
# A card as crates/op-reason-win/src/win/pass_window.rs draws it
TOAST_TEXT_X, TOAST_TEXT_W = 36.0, CARD_W - 36.0 - 12.0
TOAST_LINES = (9.0, 26.0, 43.0)  # the tops of lines 1, 2 and 3; their baselines are 13, 13 and 12 below
TOAST_BG, TOAST_GREEN, TOAST_PALE, TOAST_DIM, TOAST_BORDER = "#ffffe0", "#15803d", "#dcfce7", "#555555", "#808080"
TOAST_MORE_PAD, TOAST_MORE_RADIUS = 6.0, 4.0  # the tag +N: the room left and right of its text, its corners
# The relay window's buttons (crates/op-reason-win/src/win/window.rs)
BUTTON_H, BUTTON_TOP_GAP, BUTTON_PAD_X, BUTTON_MIN_W, BUTTON_RADIUS = 28.0, 10.0, 14.0, 96.0, 4.0
BUTTON_FILL = "#fafafa"
# The HTML page that plays the relay's scene, and what --check looks for in it
RELAY_HTML = "docs/html/relay-screens.html"


def sources() -> dict:
    """What --check expects to find in each Rust file: the copied texts, formats and sizes."""
    ui = ['"Claude Code (pid {pid})"', '"{repo} ({branch}) · {place}"']
    for t in WINDOW.values():
        ui += [f'"{v}"' for v in t.values()]
    for s, lang in SWITCH:
        ui.append(f'("{s}", ' + (f"Some(Lang::{lang.title()}))" if lang else "None)"))
    messages = [f'"{PREFIX}"']
    for lang in LANGS:
        messages += list(USAGE_SHORT[lang])
        messages.append(f'"{NO_REASON[lang]}"')
    logview = [
        f'"{CLAUDE}".to_string()',
        f'{REASON_CLIP}, "…")',
        f'{COMMAND_CLIP}, "…")',
        "display_width(c) + 2)",
        f"0x{WIDE[0][0]:X}..=0x{WIDE[0][1]:X}",
        f"0x{WIDE[-1][0]:X}..=0x{WIDE[-1][1]:X}",
    ]
    for lang, t in LOG.items():
        code = "En" if lang == "en" else "Ja"
        logview.append(", ".join(f'"{h}"' for h in t["header"]))
        logview += [f'(Lang::{code}, "{k}") => "{t[k]}"' for k in ("approved", "denied")]
        logview += [f'refused: "{t["refused"]}"', f'human: "{t["human"]}"']
        logview.append(f'(Lang::{code}, "no-reason") => "{t["no-reason"]}"')
    window = [
        f"const OUTER_WIDTH: f32 = {WIN_OUTER:.1f};",
        f"const PAD_LEFT: f32 = {PAD_LEFT:.1f};",
        f"const PAD_TOP: f32 = {PAD_TOP:.1f};",
        f"const PAD_RIGHT: f32 = {PAD_RIGHT:.1f};",
        f"const PAD_BOTTOM: f32 = {PAD_BOTTOM:.1f};",
        f"const LINE_GAP: f32 = {LINE_GAP:.1f};",
        "const SCREEN_MARGIN: f32 = 16.0;",
        f"const TITLE_SIZE: f32 = {TITLE_SIZE:.1f};",
        f"const BODY_SIZE: f32 = {BODY_SIZE:.1f};",
        f"const SWITCH_SIZE: f32 = {SWITCH_SIZE:.1f};",
        "const SWITCH_GAP: f32 = 12.0;",
        "const LIGHT_YELLOW: D2D1_COLOR_F = rgb(0xFF, 0xFF, 0xE0);",
        "const BLACK: D2D1_COLOR_F = rgb(0, 0, 0);",
        "const FIREBRICK: D2D1_COLOR_F = rgb(0xB2, 0x22, 0x22);",
        "const LINK_BLUE: D2D1_COLOR_F = rgb(0x00, 0x66, 0xCC);",
        "const GRAY: D2D1_COLOR_F = rgb(0x80, 0x80, 0x80);",
        'w!("Segoe UI")',
        'format!("{label}: ")',
        # the switch on the footer's last baseline, the footer narrower by it
        "Some((PAD_LEFT + self.width, top + self.paras[i].last_baseline))",
        "(width - taken - SWITCH_GAP).max(width / 2.0)",
    ]
    settings = [
        SETTINGS[0],
        'pub const SETTINGS_FILE: &str = "settings.json";',
        "pub const VERSION: u64 = 1;",
        'assert_eq!(render(None, Lang::Ja), r#"{"v":1,"lang":"ja"}"#);',
    ]
    relay_ui = [f'"{v}"' for t in RELAY.values() for v in t.values()]
    ui += [f'"{v}"' for t in NOTIFY.values() for v in t.values()]
    pass_toast = [
        f"pub const SHOW_MS: u64 = {SHOW_MS};",
        f"pub const FADE_MS: u64 = {FADE_MS};",
        f"pub const ENTER_MS: u64 = {ENTER_MS};",
        f"pub const APPEAR_MS: u64 = {APPEAR_MS};",
        f"pub const EVICT_MS: u64 = {EVICT_MS};",
        f"pub const BUMP_MS: u64 = {BUMP_MS};",
        f"pub const MAX_CARDS: usize = {MAX_CARDS};",
        f"pub const EDGE: f32 = {EDGE:.1f};",
        f"pub const GAP: f32 = {GAP:.1f};",
        f"pub const CARD_W: f32 = {CARD_W:.1f};",
        f"pub const CARD_H: f32 = {CARD_H:.1f};",
        f"pub const DROP: f32 = {DROP:.1f};",
        f"const EVICT_LINGER_MS: u64 = {EVICT_LINGER_MS};",
        f"pub const OPACITY: f32 = {OPACITY};",
        f"pub const MORE_GAP: f32 = {MORE_GAP:.1f};",
        f"pub const MORE_H: f32 = {MORE_H:.1f};",
        "slot_y(MAX_CARDS - 1) + CARD_H + MORE_GAP",
        "opacity: shown * OPACITY,",
        "glow: glow * shown,",
    ]
    pass_window = [
        f"const TEXT_X: f32 = {TOAST_TEXT_X:.1f};",
        "const TEXT_W: f32 = CARD_W - TEXT_X - 12.0;",
        f"const LINE1_TOP: f32 = {TOAST_LINES[0]:.1f};",
        f"const LINE2_TOP: f32 = {TOAST_LINES[1]:.1f};",
        f"const LINE3_TOP: f32 = {TOAST_LINES[2]:.1f};",
        "const BODY_LINE: (f32, f32) = (17.0, 13.0);",
        "const SMALL_LINE: (f32, f32) = (16.0, 12.0);",
        "const CHECK: (f32, f32, f32) = (18.0, 17.5, 8.0);",
        "const RADIUS: f32 = 6.0;",
        "const LIGHT_YELLOW: D2D1_COLOR_F = rgb(0xFF, 0xFF, 0xE0);",
        "const GREEN: D2D1_COLOR_F = rgb(0x15, 0x80, 0x3D);",
        "const PALE_GREEN: D2D1_COLOR_F = rgb(0xDC, 0xFC, 0xE7);",
        "const DIM: D2D1_COLOR_F = rgb(0x55, 0x55, 0x55);",
        "const GRAY: D2D1_COLOR_F = rgb(0x80, 0x80, 0x80);",
        f"const MORE_PAD: f32 = {TOAST_MORE_PAD:.1f};",
        f"const MORE_RADIUS: f32 = {TOAST_MORE_RADIUS:.1f};",
    ]
    window += [
        f"const BUTTON_H: f32 = {BUTTON_H:.1f};",
        f"const BUTTON_TOP_GAP: f32 = {BUTTON_TOP_GAP:.1f};",
        f"const BUTTON_PAD_X: f32 = {BUTTON_PAD_X:.1f};",
        f"const BUTTON_MIN_W: f32 = {BUTTON_MIN_W:.1f};",
        f"const BUTTON_RADIUS: f32 = {BUTTON_RADIUS:.1f};",
        "const BUTTON_FILL: D2D1_COLOR_F = rgb(0xFA, 0xFA, 0xFA);",
    ]
    return {
        "crates/op-reason-win/src/ui.rs": ui,
        "crates/op-reason-win/src/relay_ui.rs": relay_ui,
        "crates/op-reason-win/src/pass_toast.rs": pass_toast,
        "crates/op-reason-win/src/win/pass_window.rs": pass_window,
        "crates/op-reason-win/src/win/window.rs": window,
        "crates/op-reason-core/src/messages.rs": messages,
        "crates/op-reason-core/src/logview.rs": logview,
        "crates/op-reason-core/src/settings/file.rs": settings,
    }


# The example calls (README's usage example first)
REASONS = {
    "en": {
        "proxmox": "read the Proxmox API token to create the staging VM",
        "db": "read the database password to run the migration",
        "staging": "create the staging VM",
        "migration": "run the database migration",
    },
    "ja": {
        "proxmox": "staging の VM を作るのに Proxmox の API の token を読む",
        "db": "migration を流すのに DB の password を読む",
        "staging": "staging の VM を作る",
        "migration": "DB の migration を流す",
    },
}
PROXMOX = "op://Private/proxmox/token"
DB = "op://Private/db/password"
PROMPT_PATH = "~/src/infra"
PLACE = "example/infra (main) · ~/src/infra"
DAY = "2026-11-15"

PROMPT_BOX = {  # the 1Password approval prompt, as a plain box
    "en": {"title": "1Password approval prompt", "deny": "Deny", "approve": "Approve"},
    "ja": {"title": "1Password の承認の画面", "deny": "拒否", "approve": "許可"},
}

TEXTS = {
    "en": {
        "call.title": "op-reason: one call",
        "call.desc": (
            'Animation: in a WSL terminal, Claude Code runs op --reason "read the Proxmox API token to create the '
            'staging VM" read "op://Private/proxmox/token". op-reason\'s window opens at the top right of the screen '
            "with the time, the caller (Claude Code and its process id), the place, the reason and the command, and "
            "the 1Password approval prompt (drawn as a plain box) opens in the middle of the screen. After a click on "
            "Approve, op prints the value (hidden here), op-reason signs out and its window closes, and op-reason-log "
            "shows the call as one line: approved, by Claude, with the reason and the command."
        ),
        "call.user": "Create the staging VM on Proxmox.",
        "call.assistant": "I need the Proxmox API token first.",
        "call.hidden": "(the value, hidden here)",
        "call.steps": (
            "Claude Code calls op with a reason",
            "op-reason's window shows who calls and why, along with 1Password's approval prompt",
            "You approve, and op prints the value",
            "op-reason signs out, and its window closes",
            "Every call is in the log: op-reason-log",
        ),
        "leak.title": "One approval and the next calls: op alone and op-reason",
        "leak.desc": (
            "Animation of two screens side by side. Left, op alone from WSL: terminal 1 calls op and you approve the "
            "1Password prompt in the middle of the screen once; when terminal 2 calls op, the value comes out with no "
            "prompt at all. Right, op-reason: every call opens op-reason's window at the top right with the caller and "
            "the reason, along with the prompt, so terminal 2 is asked again and you see who asks and why."
        ),
        "leak.heads": (("op alone", "op.exe started from WSL"), ("op-reason", "a fresh op-reason.exe for every call")),
        "leak.terminal": ("Terminal 1 · Claude Code", "Terminal 2 · Claude Code"),
        "leak.approved": "approved",
        "leak.noprompt": "no prompt",
        "leak.steps": (
            (
                "1. Terminal 1 calls op: you approve the prompt",
                "2. Terminal 2 calls op: no prompt, the value comes out",
            ),
            (
                "1. Terminal 1 calls op: the window and the prompt",
                "2. Terminal 2 calls op: asked again, with who and why",
            ),
        ),
        "refuse.title": "op-reason: a call without a reason",
        "refuse.desc": (
            'Animation: op read "op://Private/proxmox/token" without --reason is refused at once, before any window '
            'or approval prompt: op-reason prints its short usage and "op-reason: There is no reason (--reason). '
            'People need it too.", and echo $? shows the exit code 2.'
        ),
        "refuse.steps": (
            "op without --reason",
            "Refused at once: no window, no approval prompt",
            "Exit code 2, and the refusal is in the log",
        ),
        "log.title": "op-reason-log",
        "log.desc": (
            "Animation: op-reason-log prints one line per call: the time, the result (approved, refused (no reason), "
            "denied), the seconds, the caller (Claude or Human), the reason and the command. The Caller, Reason and "
            "Result columns light up in turn."
        ),
        "log.steps": (
            "op-reason-log: the calls of the last hour, one line each",
            "Who: Claude Code or a person (Caller)",
            "Why: the reason given with --reason (Reason)",
            "What happened: approved, refused, denied… (Result)",
            "Who, why and the result of every call",
        ),
        "lang.title": "op-reason: choosing the language in the window",
        "lang.desc": (
            "Animation: op-reason's window opens in English, its default, at the top right of the screen, with "
            "English | 日本語 at the right of its last line, and the 1Password approval prompt (drawn as a plain box) "
            "opens in the middle of the screen. A click on 日本語 turns the window Japanese at once: 日本語 is now "
            "bold and English is the link. The prompt keeps the focus, and the choice is kept in "
            "%LOCALAPPDATA%\\op-reason\\settings.json. After the approval, the next call opens its window in Japanese."
        ),
        "lang.badge": "The prompt keeps the focus",
        "lang.saved": "Kept for the next calls",
        "lang.steps": (
            "op-reason's window is in English by default",
            "Click 日本語 at the window's bottom right",
            "The window turns Japanese at once, and the 1Password prompt keeps the focus",
            "The next call is in Japanese too: the window, the notification, the messages and op-reason-log",
        ),
        "relay.title": "op-reason's SSH relay: the window, the toasts and the notifications",
        "relay.desc": (
            "Animation of a Windows desktop. A signature that passes without a window shows a small light yellow "
            "card, a toast, at the top left, at 90% opacity so that the text behind shows through faintly: Passed · "
            "Human, the ssh command and its destination, allowed for this terminal · no reason given; it fades after "
            "four seconds. An ansible run signs eight times in one call: one card counts them (×2 … ×8), shows the last "
            "host and lights up at each. Three other calls each put a card on top and push the others down; a fourth "
            "and a fifth push the oldest out at once, and +N under the third card counts the cards pushed out (+1, "
            "+2) until four seconds after their last signature; four seconds after its last signature each card "
            "fades. Then a new call asks in op-reason's window at the top right, with its Windows notification "
            "at the bottom right, while another toast comes at the top left; a click on Allow this call closes the "
            "window and its notification together, and the call's later signatures come as one toast at the top "
            "left (×3)."
        ),
        "relay.steps": (
            "A signature that passes without a window: a toast at the top left",
            "One card per call: ×N and its last destination, four seconds again",
            "Another call's card goes on top; the others move down",
            "A fourth card pushes the oldest out at once; +N counts the cards pushed out",
            "Four seconds after its last signature, a card fades out",
            "A call that asks: the window at the top right, its notification at the bottom right, gone together",
        ),
        "relay.app": "Windows PowerShell",
    },
    "ja": {
        "call.title": "op-reason: 1 回の呼び出し",
        "call.desc": (
            "アニメーション: WSL の端末で Claude Code が "
            'op --reason "staging の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token" '
            "を打つ。画面の右上に op-reason の窓が開いて時刻・呼び手 (Claude Code と process id)・場所・理由・"
            "コマンドを出し、画面の真ん中に 1Password の承認の画面 (素の箱で描いたもの) が出る。許可を押すと "
            "op が値を出し (図では伏せた)、op-reason が sign out して窓が消え、op-reason-log にその呼び出しが "
            "1 行で出る (許可・Claude・理由・コマンド)。"
        ),
        "call.user": "Proxmox に staging の VM を作って",
        "call.assistant": "先に Proxmox の API の token が要る。",
        "call.hidden": "(値。図では伏せた)",
        "call.steps": (
            "Claude Code が理由をつけて op を呼ぶ",
            "1Password の承認の画面と一緒に、op-reason の窓が誰が・なぜを出す",
            "許可すると、op が値を出す",
            "op-reason が sign out し、窓が消える",
            "呼び出しはすべて記録に残る: op-reason-log",
        ),
        "leak.title": "1 回の承認と次の呼び出し: op だけと op-reason",
        "leak.desc": (
            "アニメーション。2 つの画面を並べる。左は WSL から op だけを使うとき: 端末 1 が op を呼び、"
            "画面の真ん中に出る 1Password の承認の画面で 1 回許可すると、端末 2 が op を呼んだときは画面なしで"
            "値が出る。右は op-reason: 呼ぶたびに承認の画面と一緒に、右上の op-reason の窓が呼び手と理由を出すので、"
            "端末 2 もまた承認を求められ、誰が・なぜが分かる。"
        ),
        "leak.heads": (("op だけ", "WSL から op.exe を起こす"), ("op-reason", "呼ぶたびに新しい op-reason.exe")),
        "leak.terminal": ("端末 1 · Claude Code", "端末 2 · Claude Code"),
        "leak.approved": "許可した",
        "leak.noprompt": "画面なし",
        "leak.steps": (
            ("1. 端末 1 が op を呼ぶ: 画面で許可する", "2. 端末 2 が op を呼ぶ: 画面なしで値が出る"),
            (
                "1. 端末 1 が op を呼ぶ: 窓と承認の画面が出る",
                "2. 端末 2 が op を呼ぶ: また承認を求め、誰が・なぜを出す",
            ),
        ),
        "refuse.title": "op-reason: 理由の無い呼び出し",
        "refuse.desc": (
            'アニメーション: 理由 (--reason) の無い op read "op://Private/proxmox/token" は、窓も承認の画面も出る前に'
            "すぐ断られる。短い使い方と「op-reason: 理由 (--reason) がありません。人が打つときも要ります。」が出て、"
            "echo $? で終了コード 2 が出る。"
        ),
        "refuse.steps": (
            "理由 (--reason) の無い op",
            "すぐ断る: 窓も承認の画面も出ない",
            "終了コードは 2。断ったことも記録に残る",
        ),
        "log.title": "op-reason-log",
        "log.desc": (
            "アニメーション: op-reason-log が 1 呼び出し 1 行で、時刻・結果 (許可・断った (理由なし)・拒否)・"
            "秒・呼び手 (Claude か人)・理由・コマンドを出す。呼び手・理由・結果の列が順に光る。"
        ),
        "log.steps": (
            "op-reason-log: 直近 1 時間の呼び出しを 1 行ずつ",
            "誰が: Claude Code か人か (呼び手)",
            "なぜ: --reason で渡した理由 (理由)",
            "どうなったか: 許可・断った・拒否… (結果)",
            "呼び出しごとに、誰が・なぜ・どうなったか",
        ),
        "lang.title": "op-reason: 窓で表示の言語を選ぶ",
        "lang.desc": (
            "アニメーション: 画面の右上に op-reason の窓が既定の英語で開き、最下行の右に English | 日本語 が出る。"
            "画面の真ん中には 1Password の承認の画面 (素の箱で描いたもの) が出る。日本語 を押すと、窓はその場で"
            "日本語に変わる (日本語 が太字になり、English が押せる側になる)。フォーカスは承認の画面のままで、"
            "選んだ言語は %LOCALAPPDATA%\\op-reason\\settings.json に残る。許可したあとの次の呼び出しは、窓が"
            "日本語で開く。"
        ),
        "lang.badge": "フォーカスは承認の画面のまま",
        "lang.saved": "次の呼び出しのために覚える",
        "lang.steps": (
            "op-reason の窓は、既定では英語で出る",
            "窓の右下の 日本語 を押す",
            "窓はその場で日本語に変わり、フォーカスは承認の画面のまま",
            "次の呼び出しも日本語: 窓・通知・端末の文・op-reason-log",
        ),
        "relay.title": "op-reason の SSH の中継: 窓・トースト・通知",
        "relay.desc": (
            "アニメーション。Windows の画面で、窓を出さずに通った署名は、左上に薄い黄色の小さなカード (トースト) を"
            "後ろの字がうっすら透ける濃さ 90% で出す: 通過 · 人、ssh のコマンドと接続先、ターミナルの許可 · 理由なし。4 秒で"
            "薄れて消える。ansible の 1 回の実行は同じ呼び出しで 8 回署名し、1 枚のカードが数 (×2 … ×8) と最後の"
            "ホストを出し、署名のたびに光る。ほかの 3 つの呼び出しはそれぞれ一番上にカードを入れてほかを下へずらし、"
            "4 枚目と 5 枚目は一番古いものをすぐ消す。消したカードは、その最後の署名から 4 秒まで 3 枚目の下の +N が"
            "数える (+1、+2)。どのカードも最後の署名から 4 秒で薄れて消える。最後に、新しい呼び出しが右上の op-reason "
            "の窓で聞き、右下に Windows "
            "の通知が出て、その間に左上には別のトーストが出る。この呼び出しを許可 を押すと窓と通知が一緒に消え、"
            "その呼び出しの続きの署名は左上の 1 枚のトースト (×3) に出る。"
        ),
        "relay.steps": (
            "窓を出さずに通った署名: 左上にトースト",
            "同じ呼び出しは 1 枚: ×N と最後の接続先、4 秒を数え直す",
            "別の呼び出しは一番上に入り、ほかは下へずれる",
            "4 枚目が来たら一番古いカードをすぐ消し、+N で数える",
            "最後の署名から 4 秒で薄れて消える",
            "聞く呼び出し: 窓は右上、通知は右下。窓と一緒に消える",
        ),
        "relay.app": "Windows PowerShell",
    },
}

# ---- small helpers ----


def num(v: float) -> str:
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("", "-0") else s


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace('"', "&quot;")


def cols(s: str) -> int:
    """Columns in a terminal, as op-reason-log counts them."""
    n = 0
    for ch in s:
        if unicodedata.category(ch) == "Cc":
            continue
        u = ord(ch)
        n += 2 if any(a <= u <= b for a, b in WIDE) else 1
    return n


def clip(s: str, n: int) -> str:
    """logview.rs: clip (the first `n` characters and "…")."""
    return s if len(s) <= n else s[:n] + "…"


def text(x: float, y: float, s: str, size: float, fill: str = INK, bold: bool = False, anchor: str = "") -> str:
    a = f' text-anchor="{anchor}"' if anchor else ""
    c = ' class="b"' if bold else ""
    return f'<text x="{num(x)}" y="{num(y)}" font-size="{num(size)}" fill="{fill}"{c}{a}>{esc(s)}</text>'


def mono(x: float, y: float, s: str, size: float, fill: str = TERM_FG, bold: bool = False) -> str:
    """Terminal text from column 0 at `x`, fitted to its columns."""
    body = s.lstrip(" ")
    x += (len(s) - len(body)) * size * ADVANCE
    body = body.rstrip(" ")
    if not body:
        return ""
    cls = "m b" if bold else "m"
    return (
        f'<text x="{num(x)}" y="{num(y)}" font-size="{num(size)}" fill="{fill}" class="{cls}" '
        f'textLength="{num(cols(body) * size * ADVANCE)}" lengthAdjust="spacing" xml:space="preserve">'
        f"{esc(body)}</text>"
    )


def runs(x: float, y: float, parts: list, size: float) -> str:
    """Terminal text in colored runs: (text, color) or (text, color, bold)."""
    out, col = [], 0
    for part in parts:
        s, fill = part[0], part[1]
        bold = len(part) > 2 and part[2]
        out.append(mono(x + col * size * ADVANCE, y, s, size, fill, bold))
        col += cols(s)
    return "".join(out)


def g(attr: str, *parts: str) -> str:
    return f"<g{attr}>{''.join(parts)}</g>"


# ---- motion (CSS @keyframes) ----


class Motion:
    """The animations of one SVG. Every animation loops over the scene's length.

    An element's own style is its look at the rest time, the frame that holds the point of the scene (the end of the
    scene unless given), so that with animations off (reduced motion) the picture stays on that frame.
    """

    def __init__(self, duration: float, rest: float | None = None) -> None:
        self.d = duration
        self.rest = duration if rest is None else rest
        self.rules: list = []
        self.names: dict = {}

    def still(self, merged: list) -> str:
        """The look at the rest time. It must not fall in a transition: there the look is between two stops."""
        for (t0, v, timing), (t1, _, _) in itertools.pairwise(merged):
            if t0 <= self.rest < t1:
                if timing != STEP:
                    raise ValueError(f"the rest time {self.rest} s falls in a transition ({num(t0)} to {num(t1)} s)")
                return v
        return merged[-1][1]

    def keyframes(self, stops: list, cls: str = "") -> str:
        """(time, declarations, timing function to the next stop), in time order, the first at 0."""
        merged: list = []
        for t, v, timing in stops:
            t = min(max(t, 0.0), self.d)
            if merged and abs(merged[-1][0] - t) < 1e-9:
                merged[-1] = (t, v, timing)
            else:
                merged.append((t, v, timing))
        if merged[-1][0] < self.d - 1e-9:
            merged.append((self.d, merged[-1][1], ""))
        body = "".join(
            f"{num(t / self.d * 100)}%{{{v}" + (f";animation-timing-function:{timing}" if timing else "") + "}"
            for t, v, timing in merged
        )
        name = self.names.get(body)
        if name is None:
            name = f"k{len(self.names) + 1}"
            self.names[body] = name
            self.rules.append(f"@keyframes {name}{{{body}}}")
            self.rules.append(f".{name}{{{self.still(merged)};animation:{name} {num(self.d)}s infinite}}")
        return f' class="{(cls + " " + name).strip()}"'

    def track(self, changes: list, ease: tuple = (), cls: str = "") -> str:
        """`changes`: (time, declarations), the first at 0. `ease`: (seconds, timing) to move between them."""
        if all(v == changes[0][1] for _, v in changes):
            c = f' class="{cls}"' if cls else ""
            return f'{c} style="{changes[0][1]}"'
        stops, prev = [(0.0, changes[0][1], STEP)], changes[0][1]
        for t, v in changes[1:]:
            if ease:
                stops.append((t, prev, ease[1]))
                stops.append((t + ease[0], v, STEP))
            else:
                stops.append((t, v, STEP))
            prev = v
        return self.keyframes(stops, cls)

    def shown(self, spans: list, fade: float = 0.0, hidden: str = HIDDEN, shown: str = SHOWN, cls: str = "") -> str:
        """Shown during each (from, to) of `spans` (`to` None: until the end)."""
        changes = [(0.0, hidden)]
        for start, end in spans:
            changes.append((start, shown))
            if end is not None:
                changes.append((end, hidden))
        if changes[1][0] <= 0:
            changes = changes[1:]
            changes[0] = (0.0, shown)
        return self.track(changes, (fade, "ease-out") if fade else (), cls)

    def at(self, start: float, end: float | None = None, fade: float = 0.0, **kw) -> str:
        return self.shown([(start, end)], fade, **kw)

    def pulse(self, at: float, length: float, on: str, off: str = HIDDEN) -> str:
        return self.keyframes([(0.0, off, STEP), (at, on, STEP), (at + length, off, STEP)])

    def band(self, start: float, end: float | None, peak: str = "opacity:1") -> str:
        """Fades in at `start` and out at `end`."""
        return self.track([(0.0, HIDDEN), (start, peak)] + ([(end, HIDDEN)] if end is not None else []), (0.25, "ease"))


# ---- parts ----


class Canvas:
    """One SVG: its size, its motion, its defs and its body."""

    def __init__(self, h: float, duration: float, rest: float | None = None) -> None:
        self.w, self.h = W, h
        self.m = Motion(duration, rest)
        self.defs: list = []
        self.body: list = []
        self.css: list = []  # rules of this scene only

    def add(self, *parts: str) -> None:
        self.body.extend(p for p in parts if p)

    def clip(self, x: float, y: float, w: float, h: float, rx: float = 0) -> str:
        cid = f"c{len(self.defs) + 1}"
        r = f' rx="{num(rx)}"' if rx else ""
        self.defs.append(
            f'<clipPath id="{cid}"><rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{num(h)}"{r}/></clipPath>'
        )
        return cid

    def render(self, title: str, desc: str) -> str:
        css = "\n".join(
            [
                f"text{{font-family:{UI}}}",
                f".m{{font-family:{MONO}}}",
                ".b{font-weight:700}",
                ".tb{transform-box:fill-box;transform-origin:50% 50%}",
                ".blink{animation:blink 1s step-end infinite}@keyframes blink{50%{opacity:0}}",
                *self.css,
                *self.m.rules,
                # reduced motion: every element keeps its own style, the frame at the rest time
                "@media (prefers-reduced-motion:reduce){*{animation:none!important}}",
            ]
        )
        defs = (
            '<linearGradient id="desk" x1="0" y1="0" x2="1" y2="1">'
            '<stop offset="0" stop-color="#4c6a8c"/><stop offset="1" stop-color="#93b2cf"/></linearGradient>'
            '<filter id="shadow" x="-10%" y="-10%" width="120%" height="140%">'
            '<feDropShadow dx="0" dy="4" stdDeviation="6" flood-color="#000" flood-opacity=".3"/></filter>'
            f'<clipPath id="canvas"><rect width="{self.w}" height="{num(self.h)}" rx="10"/></clipPath>'
            + "".join(self.defs)
        )
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{self.w}" height="{num(self.h)}" '
            f'viewBox="0 0 {self.w} {num(self.h)}" role="img" aria-labelledby="title desc">\n'
            "<!-- Made by docs/images/make_readme_svgs.py: change the script, not this file -->\n"
            f'<title id="title">{esc(title)}</title>\n'
            f'<desc id="desc">{esc(desc)}</desc>\n'
            f"<style>\n{css}\n</style>\n"
            f"<defs>{defs}</defs>\n"
            '<g clip-path="url(#canvas)">\n' + "\n".join(self.body) + "\n</g>\n</svg>\n"
        )


class Term:
    """A terminal window (Windows Terminal, dark): `frame`, then `content(...)` of lines placed by baseline and
    column."""

    def __init__(self, c: Canvas, x: float, y: float, w: float, h: float, title: str, size: float, small=False) -> None:
        self.c, self.x, self.y, self.w, self.h, self.size = c, x, y, w, h, size
        self.bar = 24 if small else 30
        self.cw = size * ADVANCE
        self.x0 = x + (12 if small else 14)
        tab_w = 172 if small else 152
        cid = c.clip(x, y, w, h, 8)
        r, tx, tt, yb = 6, x + 8, y + 5, y + self.bar
        tab = (
            f'<path d="M{num(tx)},{num(yb)}V{num(tt + r)}a{r},{r} 0 0 1 {r},-{r}H{num(tx + tab_w - r)}'
            f'a{r},{r} 0 0 1 {r},{r}V{num(yb)}Z" fill="{TERM_BG}"/>'
        )
        bx, cy = x + w, y + self.bar / 2 + 2
        buttons = (
            '<g stroke="#9a9a9a" stroke-width="1" fill="none">'
            f'<line x1="{num(bx - 78)}" y1="{num(cy)}" x2="{num(bx - 70)}" y2="{num(cy)}"/>'
            f'<rect x="{num(bx - 49.5)}" y="{num(cy - 4.5)}" width="8" height="8"/>'
            f'<path d="M{num(bx - 22)},{num(cy - 4)}l8,8M{num(bx - 14)},{num(cy - 4)}l-8,8"/></g>'
        )
        self.frame = (
            f'<rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{num(h)}" rx="8" fill="{TERM_BG}" '
            'filter="url(#shadow)"/>'
            f'<g clip-path="url(#{cid})"><rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{self.bar}" '
            f'fill="{TERM_BAR}"/>{tab}'
            + text(tx + 12, yb - (7 if small else 9), title, 11 if small else 12, "#e6e6e6")
            + buttons
            + "</g>"
            f'<rect x="{num(x + 0.5)}" y="{num(y + 0.5)}" width="{num(w - 1)}" height="{num(h - 1)}" rx="7.5" '
            'fill="none" stroke="#3a3a3a"/>'
        )
        self.content_clip = c.clip(x + 1, yb, w - 2, h - self.bar - 1)

    def col(self, n: float) -> float:
        return self.x0 + n * self.cw

    def content(self, *parts: str) -> str:
        return f'<g clip-path="url(#{self.content_clip})">{"".join(parts)}</g>'

    def line(self, y: float, parts: list, col: float = 0) -> str:
        return runs(self.col(col), y, parts, self.size)

    def prompt(self, y: float, command: str = "") -> str:
        parts = [(PROMPT_PATH, TERM_PATH), ("$ ", TERM_FG)]
        if command:
            parts.append((command, TERM_FG))
        return self.line(y, parts)

    def caret(self, y: float, col: float) -> str:
        """The block cursor at a column of the line with baseline `y`."""
        return (
            f'<rect class="blink" x="{num(self.col(col))}" y="{num(y - self.size * 0.86)}" '
            f'width="{num(self.cw)}" height="{num(self.size * 1.1)}" fill="{TERM_FG}"/>'
        )

    def band(self, y: float, col: float, ncols: float, attr: str) -> str:
        """A highlight behind columns of the line with baseline `y`."""
        return (
            f'<rect x="{num(self.col(col) - 4)}" y="{num(y - self.size * 1.02)}" width="{num(ncols * self.cw + 8)}" '
            f'height="{num(self.size * 1.42)}" rx="3" fill="{BAND}" fill-opacity=".26"{attr}/>'
        )

    def outline(self, color: str, attr: str) -> str:
        return (
            f'<rect x="{num(self.x + 1.5)}" y="{num(self.y + 1.5)}" width="{num(self.w - 3)}" '
            f'height="{num(self.h - 3)}" rx="7" fill="none" stroke="{color}" stroke-width="3"{attr}/>'
        )


def hidden_value(t: Term, y: float, col: float, n: int) -> str:
    """The value op printed, drawn as dots (the picture hides it)."""
    r = t.size * 0.22
    return "".join(
        f'<circle cx="{num(t.col(col + i + 0.5))}" cy="{num(y - t.size * 0.33)}" r="{num(r)}" fill="{TERM_FG}"/>'
        for i in range(n)
    )


def tool_dot(t: Term, y: float, color_attr: str) -> str:
    return f'<circle cx="{num(t.col(0.5))}" cy="{num(y - t.size * 0.34)}" r="{num(t.size * 0.29)}"{color_attr}/>'


def result_hook(t: Term, y: float) -> str:
    """Claude Code's corner in front of a tool's output."""
    x = t.col(2.3)
    return (
        f'<path d="M{num(x)},{num(y - t.size * 1.0)}V{num(y - t.size * 0.33)}H{num(x + t.cw * 1.4)}" fill="none" '
        f'stroke="{TERM_DIM}" stroke-width="1.2"/>'
    )


def window_layout(lines: int, footer_lines: int) -> tuple:
    """The client height of op-reason's window and the baselines of its title, its lines and the lines of its footer
    (window.rs: each paragraph is LINE_GAP below the one above, plus LINE_GAP)."""
    y, base = PAD_TOP, []
    for size, n in [(TITLE_SIZE, 1)] + [(BODY_SIZE, 1)] * lines + [(BODY_SIZE, footer_lines)]:
        y += LINE_GAP
        base += [y + size * (SEGOE_ASCENT + k * SEGOE_LINE) for k in range(n)]
        y += n * size * SEGOE_LINE + LINE_GAP
    return y + PAD_BOTTOM, base


def switch(x: float, y: float, lang: str) -> str:
    """The language switch, ending at `x` on the baseline `y`: `lang` bold and black, the other language blue and
    underlined (the link), the bar gray."""
    parts = []
    for s, code in SWITCH:
        if code is None:
            parts.append(f'<tspan fill="{WIN_BAR}">{esc(s)}</tspan>')
        elif code == lang:
            parts.append(f'<tspan class="b" fill="{WIN_INK}">{esc(s)}</tspan>')
        else:
            parts.append(f'<tspan fill="{WIN_LINK}" text-decoration="underline">{esc(s)}</tspan>')
    return (
        f'<text x="{num(x)}" y="{num(y)}" font-size="{num(SWITCH_SIZE)}" text-anchor="end" xml:space="preserve">'
        f"{''.join(parts)}</text>"
    )


def op_window(
    c: Canvas, x: float, y: float, lang: str, values: tuple, bands: tuple = (), switch_band: tuple = ()
) -> tuple:
    """op-reason's window in `lang` with its frame's top left at (x, y). `bands`: (line, from, to) to light up a line;
    `switch_band`: (from, to) to light up the language switch.

    Returns the parts, the height and the middle of 日本語 at the end of the switch (where a click switches an English
    window to Japanese).
    """
    t = WINDOW[lang]
    labels = (t["time"], t["caller"], t["place"], t["reason"], t["command"])
    footer = FOOTER_LINES[lang]
    client_h, base = window_layout(len(labels), len(footer))
    h = 1 + WIN_CAPTION + client_h + 1
    cx, cy = x + 1, y + 1 + WIN_CAPTION
    tx = cx + PAD_LEFT
    right = tx + TEXT_W
    cid = c.clip(cx, cy, WIN_W - 2, client_h)
    bx = x + WIN_W - 2 - 19
    out = [
        f'<rect x="{num(x)}" y="{num(y)}" width="{num(WIN_W)}" height="{num(h)}" fill="{WIN_CAPTION_BG}" '
        'filter="url(#shadow)"/>',
        f'<rect x="{num(cx)}" y="{num(cy)}" width="{num(WIN_W - 2)}" height="{num(client_h)}" fill="{WIN_BG}"/>',
        text(x + 10, y + 20, t["title"], 12, WIN_CAPTION_INK),
        f'<rect x="{num(bx)}" y="{num(y + 8)}" width="19" height="19" fill="{WIN_CLOSE}"/>'
        f'<path d="M{num(bx + 6.5)},{num(y + 14.5)}l6,6M{num(bx + 12.5)},{num(y + 14.5)}l-6,6" stroke="#f6f6f6" '
        'stroke-width="1.6"/>',
        f'<rect x="{num(x + 0.5)}" y="{num(y + 0.5)}" width="{num(WIN_W - 1)}" height="{num(h - 1)}" fill="none" '
        f'stroke="{WIN_BORDER}"/>',
        f'<g clip-path="url(#{cid})">',
    ]
    for line, start, end in bands:
        top = cy + base[line + 1] - BODY_SIZE * SEGOE_ASCENT
        out.append(
            f'<rect x="{num(cx + 5)}" y="{num(top - 1)}" width="{num(WIN_W - 12)}" '
            f'height="{num(BODY_SIZE * SEGOE_LINE + 2)}" rx="3" fill="{BAND}" fill-opacity=".45"'
            f"{c.m.band(start, end)}/>"
        )
    if switch_band:  # wide enough for the widest fonts' English | 日本語
        out.append(
            f'<rect x="{num(right - 104)}" y="{num(cy + base[-1] - 14)}" width="108" height="19" rx="3" fill="{BAND}" '
            f'fill-opacity=".45"{c.m.band(*switch_band)}/>'
        )
    out.append(text(tx, cy + base[0], t["title"], TITLE_SIZE, WIN_INK, True))
    for i, (label, value) in enumerate(zip(labels, values, strict=True)):
        out.append(
            f'<text x="{num(tx)}" y="{num(cy + base[i + 1])}" font-size="{num(BODY_SIZE)}" fill="{WIN_INK}">'
            f'<tspan class="b">{esc(label)}: </tspan>{esc(value)}</text>'
        )
    for k, s in enumerate(footer):
        out.append(text(tx, cy + base[len(labels) + 1 + k], s, BODY_SIZE, WIN_FOOTER, True))
    out.append(switch(right, cy + base[-1], lang))
    out.append("</g>")
    return "".join(out), h, (right - JA_WIDTH / 2, cy + base[-1] - SWITCH_SIZE * 0.38)


def mini_window(x: float, y: float, w: float, lang: str, caller: str, reason: str) -> str:
    """op-reason's window, small: its title, the caller and the reason."""
    t = WINDOW[lang]
    h = 84
    bx = x + w - 2 - 14
    return (
        f'<rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{h}" fill="{WIN_CAPTION_BG}" filter="url(#shadow)"/>'
        f'<rect x="{num(x + 1)}" y="{num(y + 21)}" width="{num(w - 2)}" height="{h - 22}" fill="{WIN_BG}"/>'
        + text(x + 8, y + 14.5, t["title"], 9.5, WIN_CAPTION_INK)
        + f'<rect x="{num(bx)}" y="{num(y + 4)}" width="14" height="14" fill="{WIN_CLOSE}"/>'
        f'<rect x="{num(x + 0.5)}" y="{num(y + 0.5)}" width="{num(w - 1)}" height="{h - 1}" fill="none" '
        f'stroke="{WIN_BORDER}"/>'
        + text(x + 11, y + 41, t["title"], 12.5, WIN_INK, True)
        + f'<text x="{num(x + 11)}" y="{num(y + 59)}" font-size="11" fill="{WIN_INK}"><tspan class="b">'
        f"{esc(t['caller'])}: </tspan>{esc(caller)}</text>"
        f'<text x="{num(x + 11)}" y="{num(y + 75)}" font-size="11" fill="{WIN_INK}"><tspan class="b">'
        f"{esc(t['reason'])}: </tspan>{esc(reason)}</text>"
    )


def button(x: float, y: float, w: float, h: float, label: str, size: float, primary: bool) -> str:
    fill, stroke, ink = (APPROVE, APPROVE, "#ffffff") if primary else ("#ffffff", "#b4b9c1", INK)
    return (
        f'<rect x="{num(x + 0.5)}" y="{num(y + 0.5)}" width="{num(w - 1)}" height="{num(h - 1)}" rx="5" '
        f'fill="{fill}" stroke="{stroke}"/>'
        + text(x + w / 2, y + h / 2 + size * 0.36, label, size, ink, True, "middle")
    )


def prompt_box(
    m: Motion, x: float, y: float, w: float, h: float, lang: str, presses: tuple, small: bool = False, focus=False
):
    """The 1Password approval prompt, as a plain box (a padlock, a title and two buttons). `focus`: ringed as the
    window that has the focus.

    Returns the parts and the center of the approve button.
    """
    p = PROMPT_BOX[lang]
    size = 11.5 if small else 13
    bw, bh = (64, 22) if small else (78, 26)
    by = y + h - bh - (8 if small else 12)
    ax = x + w - bw - (10 if small else 12)
    dx = ax - bw - 8
    lx, ly = x + (12 if small else 16), y + (8 if small else 17)
    lock = (
        f'<path d="M{num(lx + 3)},{num(ly + 6)}V{num(ly + 3.5)}a3.5,3.5 0 0 1 7,0V{num(ly + 6)}" fill="none" '
        f'stroke="{MUTED}" stroke-width="1.7"/>'
        f'<rect x="{num(lx)}" y="{num(ly + 6)}" width="13" height="10" rx="2" fill="{MUTED}"/>'
    )
    ring = (
        f'<rect x="{num(x - 2.5)}" y="{num(y - 2.5)}" width="{num(w + 5)}" height="{num(h + 5)}" rx="10" fill="none" '
        f'stroke="{APPROVE}" stroke-width="2"/>'
    )
    out = [
        f'<rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{num(h)}" rx="8" fill="#ffffff" stroke="{LINE}" '
        'filter="url(#shadow)"/>',
        ring if focus else "",
        lock,
        text(lx + 21, ly + 14, p["title"], size, INK, True),
        f'<line x1="{num(x + 1)}" y1="{num(by - (4.5 if small else 9.5))}" x2="{num(x + w - 1)}" '
        f'y2="{num(by - (4.5 if small else 9.5))}" stroke="#e5e7eb"/>',
        button(dx, by, bw, bh, p["deny"], size - 1, False),
        button(ax, by, bw, bh, p["approve"], size - 1, True),
    ]
    for t in presses:
        out.append(
            f'<rect x="{num(ax)}" y="{num(by)}" width="{num(bw)}" height="{num(bh)}" rx="5" fill="#000"'
            f"{m.pulse(t, 0.18, 'opacity:.22')}/>"
        )
    return "".join(out), (ax + bw / 2, by + bh / 2)


ARROW = (
    '<path d="M0,0V18L5,13L8.5,21L11.5,19.8L8,12H15Z" fill="#fff" stroke="#000" stroke-width="1.4" '
    'stroke-linejoin="round"/>'
)
# The hand that Windows shows over a link (op-reason's window shows it over the language it switches to), its
# fingertip at (0, 0)
HAND = (
    '<path d="M-1.8,13.4V1.8A1.8,1.8 0 0 1 1.8,1.8V8.6A1.6,1.6 0 0 1 5,8.6V9.6A1.6,1.6 0 0 1 8.2,9.6V11'
    "A1.4,1.4 0 0 1 11,11V15.6C11,18.6 9.6,21 7,21H2.6C1,21 0,20.2 -0.8,19L-5,13.9A1.3,1.3 0 0 1 -3.2,12.2Z"
    'M1.8,8.6V11.6M5,9.6V12M8.2,11V12.6" fill="#fff" stroke="#000" stroke-width="1.2" stroke-linejoin="round"/>'
)


def mouse(m: Motion, start: tuple, moves: tuple, hand: tuple = ()) -> str:
    """The mouse pointer: (from, to, where) moves. `hand`: (from, to) spans when it is the hand, not the arrow.

    Its place at the rest time is an attribute and the animation moves it from there, so that a viewer that runs no
    CSS transforms still draws it in its place.
    """
    base = start
    for _, t1, to in moves:
        if t1 <= m.rest:
            base = to

    def shift(p: tuple) -> str:
        return f"transform:translate({num(p[0] - base[0])}px,{num(p[1] - base[1])}px)"

    pos = start
    stops = [(0.0, shift(pos), STEP)]
    for t0, t1, to in moves:
        stops.append((t0, shift(pos), MOVE))
        stops.append((t1, shift(to), STEP))
        pos = to
    shape = g(m.shown(list(hand), hidden=SHOWN, shown=HIDDEN), ARROW) + g(m.shown(list(hand)), HAND) if hand else ARROW
    return f'<g transform="translate({num(base[0])},{num(base[1])})">{g(m.keyframes(stops), shape)}</g>'


def ripple(m: Motion, at: tuple, t: float) -> str:
    """A click."""
    attr = m.keyframes(
        [
            (0.0, "opacity:0;transform:scale(.45)", STEP),
            (t, "opacity:.95;transform:scale(.45)", "ease-out"),
            (t + 0.45, "opacity:0;transform:scale(2.6)", STEP),
        ],
        cls="tb",
    )
    return (
        f'<circle cx="{num(at[0])}" cy="{num(at[1])}" r="9" fill="none" stroke="{APPROVE}" stroke-width="2.5"{attr}/>'
    )


def captions(c: Canvas, y: float, h: float, steps: list) -> str:
    """The strip under a scene: the step being shown, with its number."""
    out = [f'<rect y="{num(y)}" width="{c.w}" height="{num(h)}" fill="#111827"/>']
    mid = y + h / 2
    for i, (start, s) in enumerate(steps):
        end = steps[i + 1][0] if i + 1 < len(steps) else None
        out.append(
            g(
                c.m.at(start, end, 0.2),
                f'<circle cx="28" cy="{num(mid)}" r="11" fill="#fbbf24"/>',
                text(28, mid + 4.5, str(i + 1), 12.5, "#111827", True, "middle"),
                text(48, mid + 5, s, 14, "#f9fafb"),
            )
        )
    return "".join(out)


def taskbar(y: float, h: float, clock: str) -> str:
    icons = "".join(
        f'<rect x="{num(W / 2 - 58 + i * 24)}" y="{num(y + 6)}" width="16" height="16" rx="4" fill="#fff" '
        'fill-opacity=".55"/>'
        for i in range(5)
    )
    return (
        f'<rect y="{num(y)}" width="{W}" height="{num(h)}" fill="#141820" fill-opacity=".82"/>'
        + icons
        + text(W - 14, y + 18.5, clock, 11, "#ffffff", anchor="end")
    )


def desk(x: float, y: float, w: float, h: float, rx: float = 0) -> str:
    r = f' rx="{num(rx)}"' if rx else ""
    return f'<rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{num(h)}"{r} fill="url(#desk)"/>'


def wrap(words: str, widths: tuple) -> list:
    """Wraps at spaces: line i holds at most widths[i] columns (the last width for the lines after)."""
    lines, line = [], ""
    for word in words.split(" "):
        if line and cols(f"{line} {word}") > widths[min(len(lines), len(widths) - 1)]:
            lines.append(line)
            line = word
        else:
            line = f"{line} {word}" if line else word
    return [*lines, line]


CALLS = (  # (time, result, seconds, caller, reason, op arguments): the calls of the scenes, as the log has them
    ("10:42:07", "approved", "3", "claude", "proxmox", f"read {PROXMOX}"),
    ("10:44:31", "refused", "-", "human", None, f"read {PROXMOX}"),
    ("10:46:02", "denied", "6", "claude", "db", f"read {DB}"),
)


def log_table(lang: str, calls: tuple) -> list:
    """op-reason-log's table (logview.rs): rows of (column, cell), the columns two apart."""
    t = LOG[lang]
    rows = [list(t["header"])]
    for time, result, secs, caller, reason, args in calls:
        label = f"{t['refused']} ({t['no-reason']})" if result == "refused" else t[result]
        who = CLAUDE if caller == "claude" else t["human"]
        why = clip(REASONS[lang][reason], REASON_CLIP) if reason else "-"
        rows.append([time, label, secs, who, why, clip(f"op {args}", COMMAND_CLIP)])
    widths = [max(cols(r[k]) for r in rows) for k in range(len(rows[0]))]
    starts = [sum(widths[:k]) + 2 * k for k in range(len(widths))]
    return [[(starts[k], cell) for k, cell in enumerate(r)] for r in rows]


# ---- scenes ----


def scene_call(lang: str) -> str:
    """One call: the window, the prompt, the value, the sign-out and the log."""
    tx = TEXTS[lang]
    reason = REASONS[lang]["proxmox"]
    d = 13.5
    # the screen, down to the bottom of the taskbar: tall enough for the approval prompt in its middle to clear the
    # bottom of op-reason's window (the English one, a line taller)
    screen_h = 576
    # without motion: op-reason's window and the approval prompt, before the click
    c = Canvas(screen_h + 44, d, rest=2.0)
    m = c.m
    c.add(desk(0, 0, W, screen_h), taskbar(screen_h - 28, 28, "10:42"))
    t_call, t_window, t_prompt = 0.8, 1.4, 1.55
    t_click, t_value, t_close, t_log, t_table = 5.7, 6.1, 7.5, 8.9, 9.4

    # Claude Code in a WSL terminal: the request, then the Bash tool calls op with a reason. op-reason's window (top
    # right) and the approval prompt (the middle of the screen) come in front of the terminal, so its lines stay left
    # of them: 45 columns beside the window, 37 beside the prompt (with "● Bash(" or its indent in front)
    top = 92
    command = f'op --reason "{reason}" read "{PROXMOX}")'
    tool_lines = wrap(command, (38, 38, 30))
    y_tool = [200 + 18 * i for i in range(len(tool_lines))]
    y_result = y_tool[-1] + 26
    term = Term(c, 16, top, 544, y_result + 28 - top, "Claude Code", 12)
    tool = g(
        m.at(t_call),
        tool_dot(term, y_tool[0], m.track([(0.0, f"fill:{TOOL_WAIT}"), (t_value, f"fill:{TOOL_DONE}")])),
        term.line(y_tool[0], [("Bash", TERM_FG, True), ("(" + tool_lines[0], TERM_FG)], 2),
        *(term.line(y, [(s, TERM_FG)], 7) for y, s in zip(y_tool[1:], tool_lines[1:], strict=True)),
    )
    result = g(
        m.at(t_value),
        result_hook(term, y_result),
        hidden_value(term, y_result, 5, 20),
        term.line(y_result, [(tx["call.hidden"], TERM_DIM)], 26),
    )
    c.add(
        term.frame,
        term.content(
            term.line(148, [("> ", TERM_DIM), (tx["call.user"], "#ffffff")]),
            f'<circle cx="{num(term.col(0.5))}" cy="{num(174 - 4)}" r="3.5" fill="#ffffff"/>',
            term.line(174, [(tx["call.assistant"], TERM_FG)], 2),
            tool,
            result,
        ),
    )

    # the 1Password approval prompt (a plain box) in the middle of the screen, op-reason's window at its top right
    pw, ph = 288, 96
    box, approve = prompt_box(m, (W - pw) / 2, (screen_h - ph) / 2, pw, ph, lang, (t_click,))
    c.add(g(m.at(t_prompt, t_click + 0.15, 0.2), box))
    values = (
        f"{DAY} 10:42:07",
        "Claude Code (pid 4242)",
        PLACE,
        reason,
        f"op read {PROXMOX}",
    )
    win, _, _ = op_window(c, W - WIN_RIGHT - WIN_W, 16, lang, values, bands=((1, 2.3, 3.1), (3, 3.2, 4.5)))
    hidden, shown = "opacity:0;transform:translateY(-8px)", "opacity:1;transform:translateY(0)"
    c.add(g(m.at(t_window, t_close, 0.3, hidden=hidden, shown=shown), win))

    # afterwards, op-reason-log shows the call
    ly = screen_h - 184  # its top
    log = Term(c, 16, ly, 848, 118, "Ubuntu", 10.5)
    table = log_table(lang, CALLS[:1])
    rows = [log.line(ly + 68 + 16 * i, [(cell, TERM_FG)], at) for i, row in enumerate(table) for at, cell in row]
    c.add(
        g(
            m.at(t_log, None, 0.3),
            log.frame,
            log.content(
                log.prompt(ly + 52, "op-reason-log"),
                g(m.at(t_log, t_table), log.caret(ly + 52, cols(f"{PROMPT_PATH}$ op-reason-log"))),
                g(
                    m.at(t_table),
                    log.band(ly + 84, 0, max(at + cols(cell) for at, cell in table[1]), m.band(9.7, 11.6)),
                    *rows,
                    log.prompt(ly + 100),
                    log.caret(ly + 100, cols(f"{PROMPT_PATH}$ ")),
                ),
            ),
        )
    )

    rest = screen_h - 46  # the mouse's height before and after the click, between the log and the taskbar
    c.add(
        mouse(m, (650, rest), ((4.8, 5.6, approve), (6.3, 7.1, (690, rest)))),
        ripple(m, approve, t_click),
        captions(c, screen_h, 44, list(zip((0.0, t_window, 4.8, 6.9, 8.6), tx["call.steps"], strict=True))),
    )
    return c.render(tx["call.title"], tx["call.desc"])


def scene_leak(lang: str) -> str:
    """One approval and the next call: op alone (left) and op-reason (right)."""
    tx = TEXTS[lang]
    r = REASONS[lang]
    d = 12.0
    # The two screens: op-reason's window at the top right, terminal 1, the approval prompt in the middle of the
    # screen, terminal 2
    pw, sy, sh = 422, 62, 416
    h = sy + sh + 58
    # without motion: terminal 2's call, the value without a prompt on the left, asked again on the right
    c = Canvas(h, d, rest=6.2)
    m = c.m
    c.add(f'<rect width="{W}" height="{h}" fill="#e9edf2"/>')
    calls = (  # (call, prompt and window, click, value, window closes): terminal 1, then terminal 2
        (0.6, 1.2, 2.9, 3.0, 3.6),
        (5.0, 5.5, 7.3, 7.4, 8.0),
    )
    step2 = 4.6
    for side, px in enumerate((12, 446)):
        reason = side == 1
        head, sub = tx["leak.heads"][side]
        c.add(text(px + 4, 32, head, 16, INK, True), text(px + 4, 52, sub, 12, MUTED))
        c.add(desk(px, sy, pw, sh, 8))
        presses, moves, clicks = [], [], []
        rest = (px + 384, sy + 372)
        for k, (t_call, t_ask, t_click, t_value, _) in enumerate(calls):
            ty = sy + 96 + k * 152
            term = Term(c, px + 10, ty, pw - 20, 88, tx["leak.terminal"][k], 11, small=True)
            item = PROXMOX if k == 0 else DB
            if reason:
                why = r["staging"] if k == 0 else r["migration"]
                first, second = f'op --reason "{why}"', f"read {item})"
            else:
                first, second = f"op read {item})", ""
            asked = reason or k == 0  # op alone asks once: terminal 2 gets no prompt
            value_at = t_value if asked else t_ask
            lines = [
                tool_dot(term, ty + 42, m.track([(0.0, f"fill:{TOOL_WAIT}"), (value_at, f"fill:{TOOL_DONE}")])),
                term.line(ty + 42, [("Bash", TERM_FG, True), ("(" + first, TERM_FG)], 2),
            ]
            if second:
                lines.append(term.line(ty + 58, [(second, TERM_FG)], 7))
            out_y = ty + (74 if second else 58)
            label = tx["leak.approved"] if asked else tx["leak.noprompt"]
            pill_w = cols(label) * 10 * ADVANCE + 14
            pill_x = px + pw - 22 - pill_w
            pill = (
                f'<rect x="{num(pill_x)}" y="{num(out_y - 11.5)}" width="{num(pill_w)}" height="16" rx="8" '
                f'fill="{"#374151" if asked else RED}"/>' + mono(pill_x + 7, out_y, label, 10, "#ffffff", True)
            )
            result = g(m.at(value_at), result_hook(term, out_y), hidden_value(term, out_y, 5, 12), pill)
            c.add(term.frame, term.content(g(m.at(t_call), *lines), result))
            if not asked:
                c.add(term.outline(RED, m.at(t_ask, None, 0.2)))
            if asked:
                presses.append(t_click)
                clicks.append(t_click)
                moves.append((t_click - 0.9, t_click - 0.1, None))
                moves.append((t_click + 0.4, t_click + 1.2, rest))
        # the prompt (each call that asks) and op-reason's window (each call)
        spans = [(a, v) for (_, a, _, v, _), asks in zip(calls, (True, reason), strict=True) if asks]
        bw, bh = 228, 64
        box, approve = prompt_box(m, px + (pw - bw) / 2, sy + (sh - bh) / 2, bw, bh, lang, tuple(presses), small=True)
        c.add(g(m.shown(spans, 0.2), box))
        if reason:
            for k, (_, t_ask, _, _, t_close) in enumerate(calls):
                caller = "Claude Code (pid 4242)" if k == 0 else "Claude Code (pid 5150)"
                why = r["staging"] if k == 0 else r["migration"]
                c.add(g(m.at(t_ask, t_close, 0.25), mini_window(px + pw - 10 - 300, sy + 8, 300, lang, caller, why)))
        moves = tuple((t0, t1, approve if to is None else to) for t0, t1, to in moves)
        c.add(mouse(m, rest, moves), *(ripple(m, approve, t) for t in clicks))
        before, after = tx["leak.steps"][side]
        c.add(
            g(m.at(0.0, step2, 0.2), text(px + 4, sy + sh + 26, before, 13, INK)),
            g(m.at(step2, None, 0.2), text(px + 4, sy + sh + 26, after, 13, GREEN if reason else RED, True)),
        )
    return c.render(tx["leak.title"], tx["leak.desc"])


def scene_refuse(lang: str) -> str:
    """op without a reason: refused before any window or prompt, exit code 2."""
    tx = TEXTS[lang]
    d = 9.0
    t_command, t_output, t_echo, t_code = 0.5, 1.0, 2.2, 2.6
    stderr = list(USAGE_SHORT[lang]) + [PREFIX + NO_REASON[lang]]
    size, lh = 11, 16
    first = 62
    n = 1 + len(stderr) + 3
    th = first + (n - 1) * lh + 14 - 12
    h = 12 + th + 12 + 44
    c = Canvas(h, d)
    m = c.m
    c.add(desk(0, 0, W, h - 44))
    term = Term(c, 12, 12, 856, th, "Ubuntu", size)
    command = f'op read "{PROXMOX}"'
    prompt_cols = cols(f"{PROMPT_PATH}$ ")
    y_err = [first + lh * (i + 1) for i in range(len(stderr))]
    y_echo = first + lh * (len(stderr) + 1)
    y_code, y_last = y_echo + lh, y_echo + 2 * lh
    parts = [
        term.prompt(first),
        g(m.at(0.0, t_command), term.caret(first, prompt_cols)),
        g(m.at(t_command), term.line(first, [(command, TERM_FG)], prompt_cols)),
        g(m.at(t_command, t_output), term.caret(first, prompt_cols + cols(command))),
        g(
            m.at(t_output),
            term.band(y_err[-1], 0, cols(stderr[-1]), m.band(t_output, None)),
            *(term.line(y, [(s, TERM_FG)]) for y, s in zip(y_err, stderr, strict=True)),
        ),
        g(m.at(t_echo), term.prompt(y_echo, "echo $?")),
        g(m.at(t_echo, t_code), term.caret(y_echo, prompt_cols + cols("echo $?"))),
        g(
            m.at(t_code),
            term.band(y_code, 0, 1, m.band(t_code, None)),
            term.line(y_code, [("2", "#ffffff", True)]),
            term.prompt(y_last),
            term.caret(y_last, prompt_cols),
        ),
    ]
    c.add(term.frame, term.content(*parts))
    c.add(captions(c, h - 44, 44, list(zip((0.0, t_output, t_code), tx["refuse.steps"], strict=True))))
    return c.render(tx["refuse.title"], tx["refuse.desc"])


def scene_log(lang: str) -> str:
    """op-reason-log: who, why and the result, one line per call."""
    tx = TEXTS[lang]
    d = 10.0
    size, lh, first = 10.5, 16, 62
    table = log_table(lang, CALLS)
    th = first + (len(table) + 1) * lh + 14 - 12
    h = 12 + th + 12 + 44
    c = Canvas(h, d)
    m = c.m
    c.add(desk(0, 0, W, h - 44))
    term = Term(c, 12, 12, 856, th, "Ubuntu", size)
    t_command, t_table = 0.5, 1.0
    spans = ((3, 1.8, 3.4), (4, 3.4, 5.0), (1, 5.0, 6.6))  # Caller, Reason, Result
    y_rows = [first + lh * (i + 1) for i in range(len(table))]
    y_last = first + lh * (len(table) + 1)
    bands = []
    for k, start, end in spans:
        at = table[0][k][0]
        width = max(cols(row[k][1]) for row in table)
        top, bottom = y_rows[0] - size * 1.05, y_rows[-1] + size * 0.45
        bands.append(
            f'<rect x="{num(term.col(at) - 4)}" y="{num(top)}" width="{num(width * term.cw + 8)}" '
            f'height="{num(bottom - top)}" rx="3" fill="{BAND}" fill-opacity=".26"{m.band(start, end)}/>'
        )
    prompt_cols = cols(f"{PROMPT_PATH}$ ")
    parts = [
        term.prompt(first),
        g(m.at(0.0, t_command), term.caret(first, prompt_cols)),
        g(m.at(t_command), term.line(first, [("op-reason-log", TERM_FG)], prompt_cols)),
        g(m.at(t_command, t_table), term.caret(first, prompt_cols + cols("op-reason-log"))),
        g(
            m.at(t_table),
            *bands,
            *(term.line(y, [(cell, TERM_FG)], at) for y, row in zip(y_rows, table, strict=True) for at, cell in row),
            term.prompt(y_last),
            term.caret(y_last, prompt_cols),
        ),
    ]
    c.add(term.frame, term.content(*parts))
    c.add(captions(c, h - 44, 44, list(zip((0.0, 1.8, 3.4, 5.0, 6.6), tx["log.steps"], strict=True))))
    return c.render(tx["log.title"], tx["log.desc"])


def scene_lang(lang: str) -> str:
    """The language switch: an English window, a click on 日本語, the window in Japanese at once with the focus left on
    the approval prompt, and the next call in Japanese."""
    tx = TEXTS[lang]
    r = REASONS[lang]
    d = 14.0
    screen_h = 576  # as in the one-call scene: the prompt in the middle clears the English window
    # without motion: just after the click, the window in Japanese, the prompt with the focus, the saved choice
    c = Canvas(screen_h + 44, d, rest=4.8)
    m = c.m
    c.add(desk(0, 0, W, screen_h), taskbar(screen_h - 28, 28, "10:43"))
    t_call, t_window, t_prompt = 0.6, 1.2, 1.35
    t_lang = 3.7  # the click on 日本語
    t_switch = t_lang + 0.05
    t_click, t_value, t_close = 6.6, 6.95, 7.6  # the approval of the first call
    t_call2, t_window2 = 8.6, 9.2  # the next call

    # Claude Code below: the first call, its value, then the next call
    ty = screen_h - 196
    term = Term(c, 16, ty, 848, 158, "Claude Code", 12)
    y1, y_value, y2 = ty + 54, ty + 80, ty + 108

    def bash(y: float, reason: str, item: str, done: float | None) -> str:
        changes = [(0.0, f"fill:{TOOL_WAIT}")] + ([(done, f"fill:{TOOL_DONE}")] if done else [])
        return tool_dot(term, y, m.track(changes)) + term.line(
            y, [("Bash", TERM_FG, True), (f'(op --reason "{reason}" read "{item}")', TERM_FG)], 2
        )

    c.add(
        term.frame,
        term.content(
            g(m.at(t_call), bash(y1, r["proxmox"], PROXMOX, t_value)),
            g(
                m.at(t_value),
                result_hook(term, y_value),
                hidden_value(term, y_value, 5, 20),
                term.line(y_value, [(tx["call.hidden"], TERM_DIM)], 26),
            ),
            g(m.at(t_call2), bash(y2, r["db"], DB, None)),
        ),
    )

    # the approval prompt in the middle of the screen, ringed as the window with the focus; a glow and a note on it
    # after the click on the switch
    pw, ph = 288, 96
    px, py = (W - pw) / 2, (screen_h - ph) / 2
    box, approve = prompt_box(m, px, py, pw, ph, lang, (t_click,), focus=True)
    glow = (
        f'<rect x="{num(px - 4)}" y="{num(py - 4)}" width="{num(pw + 8)}" height="{num(ph + 8)}" rx="11" fill="none" '
        f'stroke="{APPROVE}" stroke-width="8" stroke-opacity=".35"{m.band(t_switch, 5.2)}/>'
    )
    badge_w = len(tx["lang.badge"]) * (13.2 if lang == "ja" else 8) + 24  # room for the widest fonts
    bx, by = W / 2 - badge_w / 2, py + ph + 12
    badge = (
        f'<path d="M{num(W / 2 - 6)},{num(by)}l6,-6l6,6Z" fill="{APPROVE}"/>'
        f'<rect x="{num(bx)}" y="{num(by)}" width="{num(badge_w)}" height="24" rx="12" fill="{APPROVE}"/>'
        + text(W / 2, by + 16.5, tx["lang.badge"], 12.5, "#ffffff", True, "middle")
    )
    c.add(
        g(m.shown([(t_prompt, t_click + 0.15), (t_window2 + 0.15, None)], 0.2), box, glow),
        g(m.at(t_switch + 0.15, t_click, 0.2), badge),
    )

    # op-reason's window at the top right: English, then Japanese at once after the click (it keeps its top right
    # corner and takes its new height); the next call's window opens in Japanese, its switch lit up
    wx = W - WIN_RIGHT - WIN_W
    first = (f"{DAY} 10:42:07", "Claude Code (pid 4242)", PLACE, r["proxmox"], f"op read {PROXMOX}")
    second = (f"{DAY} 10:43:18", "Claude Code (pid 4242)", PLACE, r["db"], f"op read {DB}")
    win_en, _, link = op_window(c, wx, 16, "en", first)
    win_ja, _, _ = op_window(c, wx, 16, "ja", first)
    win_next, _, _ = op_window(c, wx, 16, "ja", second, switch_band=(t_window2 + 0.5, 11.6))
    slide = {"hidden": "opacity:0;transform:translateY(-8px)", "shown": "opacity:1;transform:translateY(0)"}
    c.add(
        g(m.at(t_window, t_close, 0.3, **slide), g(m.at(0.0, t_switch), win_en), g(m.at(t_switch), win_ja)),
        g(m.at(t_window2, None, 0.3, **slide), win_next),
    )

    # the choice, kept for the next calls
    card = (
        f'<rect x="16" y="16" width="318" height="70" rx="8" fill="#111827" fill-opacity=".92"/>'
        f'<path d="M30,30h12l4,4v14h-16Z" fill="none" stroke="{BAND}" stroke-width="1.6" stroke-linejoin="round"/>'
        f'<path d="M33,40l3,3l6,-6" fill="none" stroke="{BAND}" stroke-width="1.8"/>'
        + text(56, 43, tx["lang.saved"], 12.5, "#f9fafb", True)
        + mono(30, 64, SETTINGS[0], 11, "#d1d5db")
        + mono(30, 79, SETTINGS[1], 11, BAND, True)
    )
    c.add(g(m.at(t_switch + 0.6, None, 0.3), card))

    rest = (800, 300)  # the mouse before and after: on the desktop, under the window and right of the prompt
    c.add(
        mouse(
            m,
            rest,
            ((2.4, 3.2, link), (5.6, 6.4, approve), (7.0, 7.8, rest)),
            hand=((3.2, t_switch),),  # over 日本語 until the window has switched (日本語 is then the language shown)
        ),
        ripple(m, link, t_lang),
        ripple(m, approve, t_click),
        captions(c, screen_h, 44, list(zip((0.0, 2.4, t_switch, t_close + 0.2), tx["lang.steps"], strict=True))),
    )
    return c.render(tx["lang.title"], tx["lang.desc"])



# ---- the SSH relay: its window, the toasts and the notifications ----

EASE_OUT = "cubic-bezier(.33,1,.68,1)"  # pass_toast.rs eases moves out: 1 - (1 - x)^3
RELAY_CALLS = {  # the example calls of the scene: caller, command, destination, answer ("call", "terminal"), reason
    "pwsh": ("human", "ssh.exe -T git@github.com", "git@github.com", "terminal", None),
    "ansible": ("human", "ssh.exe -o ControlMaster=auto {host} \"/bin/sh -c 'echo ~'\"", "{host}", "call", "config"),
    "c1": ("claude", "ssh.exe git@github.com \"git-receive-pack 'me/r.git'\"", "git@github.com", "call", "push"),
    "c2": ("human", "ssh.exe git@github.com \"git-upload-pack 'me/infra.git'\"", "git@github.com", "terminal", None),
    "c3": ("git.exe", 'ssh.exe -o SendEnv=GIT_PROTOCOL git@github.com "git-upload-pack \'me/app.git\'"', "git@github.com",
           "call", None),
    "c4": ("human", "ssh.exe -x -oRemoteCommand=none deploy@web-01 sftp", "deploy@web-01", "terminal", None),
    "c5": ("claude", 'ssh.exe deploy@web-03 "journalctl -u app -n 50"', "deploy@web-03", "call", "logs"),
    "deploy": ("claude", 'ssh.exe deploy@web-02 "systemctl status app"', "deploy@web-02", "call", "service"),
}
RELAY_REASONS = {
    "en": {
        "config": "roll out the config",
        "push": "push the login fix",
        "service": "check that the service runs",
        "logs": "read the app's logs",
    },
    "ja": {
        "config": "設定を配る",
        "push": "ログインの修正を push する",
        "service": "サービスが動いているかを見る",
        "logs": "アプリのログを読む",
    },
}
HOST_KEY = "SHA256:x5QvN2…"


def est_width(s: str, size: float, bold: bool = False) -> float:
    """About how wide a text is in Segoe UI (Yu Gothic UI for Japanese): a width per kind of character. The scene sets
    each line's textLength to it, so that every viewer's font draws it in that room."""
    w = 0.0
    for ch in s:
        u = ord(ch)
        if any(a <= u <= b for a, b in WIDE):
            w += 1.0
        elif ch in " ·":
            w += 0.27
        elif ch in "ijlt.,:;'|!()[]/\\-":
            w += 0.3
        elif ch in "mwMW@":
            w += 0.85
        elif ch in "→×~":
            w += 0.62
        elif ch.isupper() or ch.isdigit():
            w += 0.58
        else:
            w += 0.5
    return w * size * (1.06 if bold else 1.0)


def fit(s: str, size: float, width: float, bold: bool = False) -> str:
    """`s` cut with an ellipsis to fit `width`, as DirectWrite trims the toast's lines."""
    if est_width(s, size, bold) <= width:
        return s
    while s and est_width(s + "…", size, bold) > width:
        s = s[:-1]
    return s.rstrip() + "…"


def toast_texts(lang: str, key: str, host: str = "") -> dict:
    """The toast of a signature of an example call, as relay_ui::pass_texts builds it."""
    t = RELAY[lang]
    caller, command, dest, answer, reason = RELAY_CALLS[key]
    names = {"human": NOTIFY[lang]["human"], "claude": "Claude Code"}
    why = t["pass_terminal"] if answer == "terminal" else t["pass_call"]
    return {
        "word": t["pass_word"],
        "caller": names.get(caller, caller),
        "command": command.replace("{host}", host),
        "dest": dest.replace("{host}", host),
        "why": f"{why} · {RELAY_REASONS[lang][reason] if reason else t['no_reason']}",
    }


def toast_slot(i: int) -> float:
    """pass_toast.rs: slot_y."""
    return EDGE + i * (CARD_H + GAP)


def toast_more_y() -> float:
    """pass_toast.rs: more_y (the top of the tag +N, under the third place)."""
    return toast_slot(MAX_CARDS - 1) + CARD_H + MORE_GAP


class Seg:
    """A value moving from `frm` to `to` in `dur` ms from `t`, easing out or linear (pass_toast.rs: Anim)."""

    def __init__(self, t: int, frm: float, to: float, dur: int, ease: bool) -> None:
        self.t, self.frm, self.to, self.dur, self.ease = t, frm, to, dur, ease


def seg_value(segs: list, t: int) -> float:
    started = [x for x in segs if x.t <= t]
    if not started:
        return segs[0].frm
    x = started[-1]
    if x.dur == 0 or t >= x.t + x.dur:
        return x.to
    k = (t - x.t) / x.dur
    return x.frm + (x.to - x.frm) * (1 - (1 - k) ** 3 if x.ease else k)


class SimCard:
    """A card of the simulation: its motion, its texts over time, its glows and the starts of its four seconds."""

    def __init__(self, cid: int, call: str, t: int, texts: dict, count: int) -> None:
        self.id, self.call = cid, call
        top = toast_slot(0)
        self.y = [Seg(t, top - DROP, top, ENTER_MS, True)]
        self.op = [Seg(t, 0.0, 1.0, APPEAR_MS, False)]
        self.variants = [(t, texts, count)]
        self.bumps: list = []
        self.arms = [t]
        self.fades: list = []
        self.armed = t
        self.state = ("live", 0)
        self.gone: int | None = None


def simulate_toasts(events: list, end: int) -> tuple:
    """The cards of pass_toast.rs's Stack for `events` ((ms, call, answer, texts) in time order), with their motion,
    and the cards pushed out: [call, from, until] each, counted in +N from `from` until `until`."""
    held: list = []  # holding a place, top first
    leaving: list = []
    made: list = []
    counts: dict = {}
    pushed: list = []

    def due(c: SimCard) -> int:
        kind, since = c.state
        if kind == "live":
            return c.armed + SHOW_MS
        if kind == "fading":
            return since + FADE_MS
        return since + EVICT_MS + EVICT_LINGER_MS

    def layout(t: int) -> None:
        for i, c in enumerate(held):
            to = toast_slot(i)
            if c.y[-1].to != to:
                c.y.append(Seg(t, seg_value(c.y, t), to, ENTER_MS, True))

    def advance(now: int) -> None:
        while True:
            due_now = [(due(c), 0, i) for i, c in enumerate(held)] + [(due(c), 1, i) for i, c in enumerate(leaving)]
            due_now = [d for d in due_now if d[0] <= now]
            if not due_now:
                return
            t, is_leaving, i = min(due_now, key=lambda d: d[0])
            if is_leaving:
                leaving.pop(i).gone = t
                continue
            c = held[i]
            if c.state[0] == "live":
                c.state = ("fading", t)
                c.op.append(Seg(t, seg_value(c.op, t), 0.0, FADE_MS, False))
                c.fades.append(t)
            else:
                held.pop(i).gone = t
                layout(t)

    for t, call, answer, texts in events:
        advance(t)
        n = counts[(call, answer)] = counts.get((call, answer), 0) + 1
        hit = next((c for c in held if c.call == call), None)
        if hit:
            hit.variants.append((t, texts, n))
            hit.bumps.append(t)
            hit.arms.append(t)
            hit.armed = t
            if hit.state[0] == "fading":
                hit.op.append(Seg(t, seg_value(hit.op, t), 1.0, APPEAR_MS, False))
            hit.state = ("live", 0)
            continue
        # a call pushed out comes back as a new card
        for q in pushed:
            if q[0] == call and q[1] <= t < q[2]:
                q[2] = t
        card = SimCard(len(made) + 1, call, t, texts, n)
        made.append(card)
        held.insert(0, card)
        while len(held) > MAX_CARDS:
            # the card whose last signature is the oldest (of two, the lower one); never the new one at the top
            old = held.pop(min(range(len(held) - 1, 0, -1), key=lambda k: held[k].armed))
            if old.state[0] == "live":
                pushed.append([old.call, t, old.armed + SHOW_MS])
            old.op.append(Seg(t, seg_value(old.op, t), 0.0, EVICT_MS, False))
            old.state = ("leaving", t)
            leaving.append(old)
        layout(t)
    advance(end)
    return made, pushed


def more_steps(pushed: list) -> list:
    """(ms, N): the count +N from that time on (pass_toast.rs: Stack::more)."""
    steps: list = []
    for t in sorted({q[1] for q in pushed} | {q[2] for q in pushed}):
        n = sum(1 for q in pushed if q[1] <= t < q[2])
        if not steps or steps[-1][1] != n:
            steps.append((t, n))
    return steps


def seg_stops(segs: list, fmt) -> list:
    """Keyframe stops (seconds) of a value made of segments; a segment that the next one starts in is cut short."""
    stops = [(0.0, fmt(segs[0].frm), STEP)]
    for k, x in enumerate(segs):
        nxt = segs[k + 1].t if k + 1 < len(segs) else None
        stops.append((x.t / 1000, fmt(x.frm), (EASE_OUT if x.ease else "linear") if x.dur else STEP))
        if x.dur and (nxt is None or x.t + x.dur <= nxt):
            stops.append(((x.t + x.dur) / 1000, fmt(x.to), STEP))
    return stops


def glow_value(ms: float) -> float:
    """pass_toast.rs: glow."""
    if ms < 0 or ms >= BUMP_MS:
        return 0.0
    p = ms / BUMP_MS
    return p / 0.25 if p < 0.25 else 1 - (p - 0.25) / 0.75


def toast_card(c: Canvas, card: SimCard) -> str:
    """A card of the simulation where pass_window.rs draws it, moving and fading as simulated: shadow, glow, light
    yellow face with a gray border, the green check, its texts over time and the line of its four seconds (in steps of
    half a second). The card is at 90% (an inner group) times its fade (the outer); the ring of its glow is outside the
    90%, at full strength, and fades with the card."""
    m = c.m
    rest = round(m.rest * 1000)
    shown = card.y[0].t <= rest and (card.gone is None or card.gone > rest)
    rest_y = seg_value(card.y, rest) if shown else toast_slot(0)
    x = EDGE
    ystops = seg_stops(card.y, lambda v: f"transform:translateY({num(v - rest_y)}px)")
    ostops = seg_stops(card.op, lambda v: f"opacity:{num(v)}")
    if card.gone is not None:
        ostops.append((card.gone / 1000, "opacity:0", STEP))
    if card.bumps:
        glows = [(0.0, 0.0, STEP)]
        for k, b in enumerate(card.bumps):
            nxt = card.bumps[k + 1] if k + 1 < len(card.bumps) else b + BUMP_MS
            peak, end = min(b + BUMP_MS / 4, nxt), min(b + BUMP_MS, nxt)
            glows += [
                (b / 1000, 0.0, "linear"),
                (peak / 1000, glow_value(peak - b), "linear"),
                (end / 1000, glow_value(end - b) if end < b + BUMP_MS else 0.0, STEP),
            ]
        glow_attr = m.keyframes([(t, f"opacity:{num(v)}", e) for t, v, e in glows])
    else:
        glow_attr = ' style="opacity:0"'
    ends = sorted([*card.arms[1:], *card.fades, *([card.gone] if card.gone is not None else [])])
    bar = [(0.0, "transform:scaleX(0)")]
    for a in card.arms:
        end = min((e for e in ends if e > a), default=a + SHOW_MS)
        k = 0
        while a + k * 500 < end and k * 500 < SHOW_MS:
            bar.append(((a + k * 500) / 1000, f"transform:scaleX({num(1 - k * 500 / SHOW_MS)})"))
            k += 1
        bar.append((end / 1000, "transform:scaleX(0)"))
    lx = x + TOAST_TEXT_X
    parts = [
        f'<rect x="{num(x - 1)}" y="1" width="{num(CARD_W + 2)}" height="{num(CARD_H + 2)}" rx="7" fill="#000" '
        'fill-opacity=".16"/>',
        f'<rect x="{num(x)}" y="0" width="{num(CARD_W)}" height="{num(CARD_H)}" rx="6" fill="{TOAST_BG}"/>',
        f'<rect x="{num(x + 0.5)}" y=".5" width="{num(CARD_W - 1)}" height="{num(CARD_H - 1)}" rx="6" fill="none" '
        f'stroke="{TOAST_BORDER}" stroke-opacity=".8"/>',
        f'<circle cx="{num(x + 18)}" cy="17.5" r="8" fill="{TOAST_GREEN}"/>',
        f'<path d="M{num(x + 14)},17.8L{num(x + 16.8)},20.6L{num(x + 22.2)},14.5" fill="none" stroke="#fff" '
        'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>',
        f'<rect x="{num(lx)}" y="{num(CARD_H - 5)}" width="{num(TOAST_TEXT_W)}" height="2" fill="{TOAST_GREEN}" '
        f'fill-opacity=".9"{m.track(bar, cls="tl")}/>',
    ]
    for k, (t, texts, n) in enumerate(card.variants):
        until = card.variants[k + 1][0] if k + 1 < len(card.variants) else card.gone
        head_w = est_width(f"{texts['word']} · {texts['caller']}", BODY_SIZE, True)
        line1 = (
            f'<text x="{num(lx)}" y="22" font-size="{num(BODY_SIZE)}" textLength="{num(head_w)}" lengthAdjust="spacing" '
            f'xml:space="preserve"><tspan class="b" fill="{TOAST_GREEN}">{esc(texts["word"])}</tspan>'
            f'<tspan class="b" fill="#000"> · {esc(texts["caller"])}</tspan></text>'
        )
        chip = ""
        if n >= 2:
            label = f"×{n}"
            cx, cw = lx + head_w + 6, est_width(label, 11, True)
            chip = (
                f'<rect x="{num(cx)}" y="10" width="{num(cw + 10)}" height="15" rx="4" fill="{TOAST_PALE}"/>'
                f'<text x="{num(cx + 5)}" y="22" font-size="11" fill="{TOAST_GREEN}" class="b" textLength="{num(cw)}" '
                f'lengthAdjust="spacing">{esc(label)}</text>'
            )
        dest = f" → {texts['dest']}"
        dest_w = est_width(dest, BODY_SIZE)
        command = fit(texts["command"], BODY_SIZE, TOAST_TEXT_W - dest_w)
        line2_w = est_width(command, BODY_SIZE) + dest_w
        why = fit(texts["why"], 11, TOAST_TEXT_W)
        lines = (
            line1
            + chip
            + f'<text x="{num(lx)}" y="39" font-size="{num(BODY_SIZE)}" fill="#000" textLength="{num(line2_w)}" '
            f'lengthAdjust="spacing" xml:space="preserve">{esc(command + dest)}</text>'
            + f'<text x="{num(lx)}" y="55" font-size="11" fill="{TOAST_DIM}" textLength="{num(est_width(why, 11))}" '
            f'lengthAdjust="spacing" xml:space="preserve">{esc(why)}</text>'
        )
        parts.append(g(m.at(t / 1000, None if until is None else until / 1000), lines))
    ring = (
        f'<rect x="{num(x - 2.5)}" y="-2.5" width="{num(CARD_W + 5)}" height="{num(CARD_H + 5)}" rx="8.5" fill="none" '
        f'stroke="{TOAST_GREEN}" stroke-width="2.5" stroke-opacity=".9"{glow_attr}/>'
    )
    inner = g(m.keyframes(ostops), g(f' style="opacity:{num(OPACITY)}"', *parts), ring)
    return f'<g transform="translate(0,{num(rest_y)})">{g(m.keyframes(ystops), inner)}</g>'


def more_tag(c: Canvas, steps: list) -> str:
    """The tag +N where pass_window.rs draws it (under the third place, at 90%), one per value of N while it holds."""
    m = c.m
    x, top = EDGE, toast_more_y()
    out = []
    for k, (t, n) in enumerate(steps):
        if n == 0:
            continue
        until = steps[k + 1][0] if k + 1 < len(steps) else None
        label = f"+{n}"
        tw = est_width(label, 11, True)
        w = tw + 2 * TOAST_MORE_PAD
        r = TOAST_MORE_RADIUS
        out.append(
            g(
                m.at(t / 1000, None if until is None else until / 1000),
                f'<rect x="{num(x - 1)}" y="{num(top + 1)}" width="{num(w + 2)}" height="{num(MORE_H + 1)}" '
                f'rx="{num(r + 1)}" fill="#000" fill-opacity=".16"/>',
                f'<rect x="{num(x)}" y="{num(top)}" width="{num(w)}" height="{num(MORE_H)}" rx="{num(r)}" '
                f'fill="{TOAST_BG}"/>',
                f'<rect x="{num(x + 0.5)}" y="{num(top + 0.5)}" width="{num(w - 1)}" height="{num(MORE_H - 1)}" '
                f'rx="{num(r)}" fill="none" stroke="{TOAST_BORDER}" stroke-opacity=".8"/>',
                f'<text x="{num(x + TOAST_MORE_PAD)}" y="{num(top + 12)}" font-size="11" fill="{TOAST_DIM}" class="b" '
                f'textLength="{num(tw)}" lengthAdjust="spacing">{esc(label)}</text>',
            )
        )
    return g(f' style="opacity:{num(OPACITY)}"', *out) if out else ""


def relay_window(c: Canvas, x: float, y: float, lang: str, values: tuple, press: float) -> tuple:
    """The relay's window that asks (per-call), with its frame's top left at (x, y), as window.rs draws it with the
    texts of relay_ui.rs: the title, six of its lines, the footer and the language switch, and its row of buttons
    (Allow this call at the left, Deny at the right). Returns the parts, the height and the middle of Allow."""
    t = RELAY[lang]
    labels = (t["time"], t["caller"], t["reason"], t["command"], t["destination"], t["purpose"])
    text_h, base = window_layout(len(labels), 1)
    client_h = text_h + BUTTON_TOP_GAP + BUTTON_H
    h = 1 + WIN_CAPTION + client_h + 1
    cx, cy = x + 1, y + 1 + WIN_CAPTION
    tx = cx + PAD_LEFT
    right = tx + TEXT_W
    bx = x + WIN_W - 2 - 19
    out = [
        f'<rect x="{num(x)}" y="{num(y)}" width="{num(WIN_W)}" height="{num(h)}" fill="{WIN_CAPTION_BG}" '
        'filter="url(#shadow)"/>',
        f'<rect x="{num(cx)}" y="{num(cy)}" width="{num(WIN_W - 2)}" height="{num(client_h)}" fill="{WIN_BG}"/>',
        text(x + 10, y + 20, t["title"], 12, WIN_CAPTION_INK),
        f'<rect x="{num(bx)}" y="{num(y + 8)}" width="19" height="19" fill="{WIN_CLOSE}"/>'
        f'<path d="M{num(bx + 6.5)},{num(y + 14.5)}l6,6M{num(bx + 12.5)},{num(y + 14.5)}l-6,6" stroke="#f6f6f6" '
        'stroke-width="1.6"/>',
        f'<rect x="{num(x + 0.5)}" y="{num(y + 0.5)}" width="{num(WIN_W - 1)}" height="{num(h - 1)}" fill="none" '
        f'stroke="{WIN_BORDER}"/>',
        text(tx, cy + base[0], t["title"], TITLE_SIZE, WIN_INK, True),
    ]
    for i, (label, value) in enumerate(zip(labels, values, strict=True)):
        out.append(
            f'<text x="{num(tx)}" y="{num(cy + base[i + 1])}" font-size="{num(BODY_SIZE)}" fill="{WIN_INK}">'
            f'<tspan class="b">{esc(label)}: </tspan>{esc(value)}</text>'
        )
    footer = t["footer"].replace("{0}", "60")
    footer_w = min(est_width(footer, BODY_SIZE, True), TEXT_W - 79 - 12)  # window.rs: narrower by the switch
    out.append(
        f'<text x="{num(tx)}" y="{num(cy + base[-1])}" font-size="{num(BODY_SIZE)}" fill="{WIN_FOOTER}" class="b" '
        f'textLength="{num(footer_w)}" lengthAdjust="spacingAndGlyphs">{esc(footer)}</text>'
    )
    out.append(switch(right, cy + base[-1], lang))
    top = cy + client_h - PAD_BOTTOM - BUTTON_H
    allow_w = max(est_width(t["allow"], BODY_SIZE) + 2 * BUTTON_PAD_X, BUTTON_MIN_W)
    deny_w = max(est_width(t["deny"], BODY_SIZE) + 2 * BUTTON_PAD_X, BUTTON_MIN_W)
    for left, w, label, edge, ink in (
        (tx, allow_w, t["allow"], WIN_BAR, WIN_INK),
        (right - deny_w, deny_w, t["deny"], WIN_FOOTER, WIN_FOOTER),
    ):
        out.append(
            f'<rect x="{num(left + 0.5)}" y="{num(top + 0.5)}" width="{num(w - 1)}" height="{num(BUTTON_H - 1)}" '
            f'rx="{num(BUTTON_RADIUS)}" fill="{BUTTON_FILL}" stroke="{edge}"/>'
            + text(left + w / 2, top + BUTTON_H / 2 + BODY_SIZE * 0.36, label, BODY_SIZE, ink, anchor="middle")
        )
    out.append(
        f'<rect x="{num(tx + 1)}" y="{num(top + 1)}" width="{num(allow_w - 2)}" height="{num(BUTTON_H - 2)}" '
        f'rx="{num(BUTTON_RADIUS)}" fill="#000"{c.m.pulse(press, 0.25, "opacity:.16")}/>'
    )
    return "".join(out), h, (tx + allow_w / 2, top + BUTTON_H / 2)


def notification(x: float, y: float, w: float, lang: str, lines: tuple) -> str:
    """A Windows notification (light, as Windows 11 draws one): the app's name, the title and two lines."""
    app = TEXTS[lang]["relay.app"]
    body = [fit(s, 12, w - 28) for s in lines[1:]]
    return (
        f'<rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="96" rx="8" fill="#f9f9f9" stroke="#d6d6d6" '
        'filter="url(#shadow)"/>'
        f'<rect x="{num(x + 14)}" y="{num(y + 12)}" width="13" height="13" rx="2.5" fill="#2671be"/>'
        f'<path d="M{num(x + 17)},{num(y + 15.5)}l3,3l-3,3" fill="none" stroke="#fff" stroke-width="1.4"/>'
        + text(x + 34, y + 22.5, app, 10.5, "#5f5f5f")
        + text(x + 14, y + 46, fit(lines[0], 12.5, w - 28, True), 12.5, INK, True)
        + text(x + 14, y + 65, body[0], 12, "#333333")
        + text(x + 14, y + 83, body[1], 12, "#333333")
    )


def editor_window(x: float, y: float, w: float, h: float) -> str:
    """An app on the desktop, under everything (dark, its text drawn as bars)."""
    bars = "".join(
        f'<rect x="{num(x + 14)}" y="{num(y + 54 + 16 * i)}" width="{num((w - 28) * (0.72, 0.54, 0.86)[i % 3])}" '
        'height="7" rx="3.5" fill="#fff" fill-opacity=".08"/>'
        for i in range(int((h - 70) // 16))
    )
    return (
        f'<rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{num(h)}" rx="8" fill="#262b31" stroke="#3c444c" '
        'filter="url(#shadow)"/>'
        f'<path d="M{num(x)},{num(y + 30)}V{num(y + 8)}a8,8 0 0 1 8,-8H{num(x + w - 8)}a8,8 0 0 1 8,8V{num(y + 30)}Z" '
        'fill="#31373e"/>'
        f'<line x1="{num(x)}" y1="{num(y + 40)}" x2="{num(x + w)}" y2="{num(y + 40)}" stroke="#3c444c"/>' + bars
    )


def scene_relay(lang: str) -> str:
    """The SSH relay: toasts at the top left (one per call, three at most, four seconds), and the window that asks at
    the top right with its notification at the bottom right, gone together."""
    tx = TEXTS[lang]
    r = RELAY[lang]
    d = 33.0
    screen_h = 470
    # without motion: the window asks, its notification is at the bottom right, and a toast is at the top left
    c = Canvas(screen_h + 44, d, rest=24.4)
    c.css.append(".tl{transform-box:fill-box;transform-origin:0 50%}")
    m = c.m
    c.add(desk(0, 0, W, screen_h), editor_window(80, 92, 440, 270), taskbar(screen_h - 28, 28, "10:42"))
    events = [(300, "pwsh", "a1", toast_texts(lang, "pwsh"))]
    events += [(6000 + 300 * i, "ansible", "a2", toast_texts(lang, "ansible", f"web-{i + 1:02d}")) for i in range(8)]
    events += [(t, k, f"a-{k}", toast_texts(lang, k)) for t, k in ((13200, "c1"), (14000, "c2"), (14800, "c3"))]
    # a fourth and a fifth: c1 and c2 are pushed out, +1 and +2, until their four seconds have passed
    events += [(t, k, f"a-{k}", toast_texts(lang, k)) for t, k in ((16000, "c4"), (16400, "c5"))]
    # a new command typed in the first terminal: another call, on the same answer for the terminal
    events += [(23400, "pwsh2", "a1", toast_texts(lang, "pwsh"))]
    events += [(t, "deploy", "a-deploy", toast_texts(lang, "deploy")) for t in (26700, 27100, 27500)]
    t_open, t_notify, t_press, t_close = 22.8, 22.9, 25.5, 26.1

    # the window that asks, at the top right (it opens and closes with a slide), and its notification
    reason = RELAY_REASONS[lang]["service"]
    command = RELAY_CALLS["deploy"][1]
    values = (
        f"{DAY} 10:42:07",
        r["claude_pid"].replace("{0}", "4242"),
        reason,
        command,
        f"deploy@web-02 · {r['host_key'].replace('{0}', HOST_KEY)}",
        r["userauth"].replace("{0}", "deploy"),
    )
    win, _, allow = relay_window(c, W - WIN_RIGHT - WIN_W, 16, lang, values, t_press)
    slide = {"hidden": "opacity:0;transform:translateY(-8px)", "shown": "opacity:1;transform:translateY(0)"}
    c.add(g(m.at(t_open, t_close, 0.3, **slide), win))
    nw = 320
    note = notification(
        W - 12 - nw,
        screen_h - 28 - 12 - 96,
        nw,
        lang,
        (r["title"], NOTIFY[lang]["toast_reason"].replace("{0}", reason), f"Claude Code · {command}"),
    )
    c.add(
        g(
            m.at(t_notify, t_close, 0.3, hidden="opacity:0;transform:translateX(24px)", shown="opacity:1;transform:translateX(0)"),
            note,
        )
    )

    # the toasts, drawn last so that they are on top
    cards, pushed = simulate_toasts(events, round(d * 1000))
    for card in cards:
        c.add(toast_card(c, card))
    c.add(more_tag(c, more_steps(pushed)))

    rest = (500, 410)
    c.add(
        mouse(m, rest, ((24.6, 25.3, allow), (25.9, 26.6, rest))),
        ripple(m, allow, t_press),
        captions(
            c,
            screen_h,
            44,
            list(zip((0.0, 5.6, 13.0, 15.8, 18.4, 22.4), tx["relay.steps"], strict=True)),
        ),
    )
    return c.render(tx["relay.title"], tx["relay.desc"])


def relay_html_needles() -> list:
    """What the HTML page must hold: the times and counts of pass_toast.rs, and the texts of the toast and the window."""
    times = [
        f"SHOW = {SHOW_MS}",
        f"FADE = {FADE_MS}",
        f"ENTER = {ENTER_MS}",
        f"APPEAR = {APPEAR_MS}",
        f"EVICT = {EVICT_MS}",
        f"BUMP = {BUMP_MS}",
        f"MAX = {MAX_CARDS}",
        f"GAP = {GAP:g}",
        f"EDGE = {EDGE:g}",
        f"OPACITY = {OPACITY:g}",
        f"MORE_GAP = {MORE_GAP:g}",
        f"MORE_H = {MORE_H:g}",
        f"MORE_PAD = {TOAST_MORE_PAD:g}",
    ]
    keys = ("title", "pass_word", "pass_call", "pass_terminal", "no_reason", "allow", "deny")
    return times + [RELAY[lang][k] for lang in LANGS for k in keys]


SCENES = (
    ("readme-1-call", scene_call),
    ("readme-2-approval-leak", scene_leak),
    ("readme-3-no-reason", scene_refuse),
    ("readme-4-log", scene_log),
    ("readme-5-language", scene_lang),
    ("readme-6-relay-screens", scene_relay),
)


def build() -> dict:
    return {HERE / f"{name}.{lang}.svg": make(lang) for name, make in SCENES for lang in LANGS}


def check_sources() -> list:
    problems = []
    for rel, needles in sources().items():
        body = (ROOT / rel).read_text(encoding="utf-8")
        problems += [f"{rel} no longer has {n!r}" for n in needles if n not in body]
    problems += [
        f"FOOTER_LINES[{lang!r}] is not the footer of WINDOW[{lang!r}]"
        for lang in LANGS
        if " ".join(FOOTER_LINES[lang]) != WINDOW[lang]["footer"]
    ]
    page = ROOT / RELAY_HTML
    if not page.is_file():
        problems.append(f"{RELAY_HTML} is missing")
    else:
        body = page.read_text(encoding="utf-8")
        problems += [f"{RELAY_HTML} no longer has {n!r}" for n in relay_html_needles() if n not in body]
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--check", action="store_true", help="compare the SVGs with the script's output instead")
    args = parser.parse_args()
    problems = check_sources()
    files = build()
    if args.check:
        for path, svg in files.items():
            rel = path.relative_to(ROOT)
            if not path.is_file():
                problems.append(f"{rel} is missing")
            elif path.read_text(encoding="utf-8") != svg:
                problems.append(f"{rel} differs from the script's output")
        for p in problems:
            print(f"make_readme_svgs.py: {p}", file=sys.stderr)
        if problems:
            print(
                "make_readme_svgs.py: change the script (the copies of the Rust texts too), then run "
                "python3 docs/images/make_readme_svgs.py",
                file=sys.stderr,
            )
            return 1
        print(f"make_readme_svgs.py: {len(files)} SVGs match the script")
        return 0
    for p in problems:
        print(f"make_readme_svgs.py: warning: {p}", file=sys.stderr)
    for path, svg in files.items():
        path.write_text(svg, encoding="utf-8", newline="\n")
        print(f"{path.relative_to(ROOT)}: {len(svg.encode())} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
