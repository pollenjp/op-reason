#!/usr/bin/env python3
"""Makes the animations of the READMEs (docs/images/readme-*.svg), in English and Japanese.

- One SVG per scene and language. They move with CSS @keyframes only: GitHub shows README images without running
  scripts.
- Each SVG has its own background, so that it reads the same on GitHub's light and dark themes. It uses the
  viewer's fonts (an image cannot load web fonts).
- With reduced motion (prefers-reduced-motion: reduce), each SVG holds still on the frame that carries its point
  (scene 1: op-reason's window and the approval prompt), so that nothing is lost without the motion. Viewers
  that run no CSS animations show the same frame.
- op-reason's window is laid out as crates/op-reason-win/src/win/window.rs draws it (the header, the reason, the chips,
  the command's box or the relay's process tree with the box under its row, the band with its picture of the screen,
  the footer and the language switch's pill), with the texts
  of crates/op-reason-win/src/ui.rs, in the dark look of crates/op-reason-win/src/theme.rs (the windows follow Windows'
  app mode; the animations show the dark one). The refusal and the log table are op-reason's output
  (the texts of crates/op-reason-core/src/messages.rs and logview.rs), the saved choice of a language is the file of
  crates/op-reason-core/src/settings/file.rs. The 1Password approval prompt is a plain box: it copies neither the look
  nor the words of 1Password's screen.
- The SSH relay's scene: its window that asks with the texts of crates/op-reason-win/src/relay_ui.rs (the call's
  process tree, its countdown ticking), the toasts of signatures that passed without a window as
  crates/op-reason-win/src/win/pass_window.rs draws them in the same dark look, moving by the rules and
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
import math
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

# op-reason's window: the layout (DIPs) of crates/op-reason-win/src/win/window.rs (build). What Windows draws around
# it follows design 2026-11-26-modern-window-design.md §4.1: round corners, and a caption in the window's background
# color that shows its close button only (no title, no icon).
WIN_OUTER = 520.0  # OUTER_WIDTH
WIN_W = WIN_OUTER - 2 * 7  # the frame as seen: Windows draws nothing in the outer 7 DIPs on both sides
WIN_RIGHT = 16 + 8  # SCREEN_MARGIN, then the invisible part of the frame on the right
WIN_TOP = 16.0  # SCREEN_MARGIN
WIN_CAPTION = 30.0  # the caption of the tool window, below the top border line
WIN_RADIUS = 8.0  # Windows 11's round corners (DWMWCP_ROUND)
WIN_BORDER = "#454545"  # the line Windows draws around a dark window
WIN_CLOSE = "#c8c8c8"  # the close button's glyph: the only thing the caption shows
PAD_LEFT, PAD_TOP, PAD_RIGHT, PAD_BOTTOM = 14.0, 12.0, 14.0, 11.0
TEXT_W = WIN_W - 2 - PAD_LEFT - PAD_RIGHT  # 476: the client area (the frame less its border lines) less the paddings
SECTION_GAP = 14.0  # between the parts of the window
TILE, TILE_RADIUS, TILE_GAP = 28.0, 7.0, 10.0  # the header's tile
TITLE_SIZE, REASON_SIZE, CODE_SIZE = 14.0, 17.0, 12.0  # the other texts are 12, 11 and 10 (design §4.5)
CHIP_H, CHIP_PAD, CHIP_MARK, CHIP_GAP = 24.0, 9.0, 16.0, 6.0
CODE_PAD_X, CODE_PAD_Y, CODE_RADIUS = 10.0, 8.0, 6.0  # the command's box
MARK_W = 18.0  # the column of a mark before a line (the purpose, the footer's shield)
BAND_PAD_X, BAND_PAD_Y, BAND_RADIUS = 10.0, 8.0, 8.0
SKETCH = (64.0, 46.0)  # the band's picture of the screen
PILL_H, PILL_PAD, PILL_GAP = 24.0, 9.0, 12.0  # the language switch
BAR_H = 4.0  # the bar of the seconds left to answer
BUTTON_H, BUTTON_TOP_GAP, BUTTON_GAP, BUTTON_PAD_X, BUTTON_MIN_W, BUTTON_RADIUS = 30.0, 14.0, 8.0, 14.0, 96.0, 6.0
# The relay's process tree (design 2026-11-27-relay-process-tree-design.md §3.5): how far each level indents, the height
# of a row's line (2 more above and below it), where a row's text starts after its dot or elbow, the height of its
# pills, how far the boxes under the rows end before the right edge (inside the frames), the gap between a row's parts
TREE_INDENT, TREE_LINE, TREE_TEXT, TREE_PILL_H, TREE_INSET, TREE_GAP = 16.0, 16.0, 14.0, 18.0, 14.0, 6.0
COUNTDOWN_MS = 250  # the window redraws its countdown this often
SEGOE_ASCENT = 2210 / 2048  # Segoe UI: DirectWrite puts the baseline this far (em) below the top of a line,
SEGOE_LINE = (2210 + 514) / 2048  # and a line is this tall
MONO_ASCENT, MONO_LINE = 0.93, 1.17  # about the same for Cascadia Mono (the command's box, the toasts' line 2)

# The dark look (crates/op-reason-win/src/theme.rs: DARK): the colors of the windows and the toasts by role. --check
# looks for each in the DARK palette.
DARK = {
    "background": "#202020",
    "text": "#ffffff",
    "secondary": "#a0a0a0",
    "tertiary": "#8a8a8a",
    "surface": "#2b2b2b",
    "line": "#3c3c3c",
    "code_bg": "#141414",
    "code_border": "#333333",
    "code_text": "#d4d4d4",
    "code_path": "#e5b07b",
    "accent": "#60cdff",
    "on_accent": "#000000",
    "caution": "#fce100",
    "on_caution": "#202020",
    "critical": "#ff99a4",
    "success": "#6ccb5f",
    "on_success": "#000000",
    "success_tint": "#1f3a1c",
    "claude": "#c3a6ff",
    "band_bg": "#1b2a36",
    "band_border": "#2d4b63",
    "band_title": "#e6f3fb",
    "band_text": "#9cc9e8",
    "pic_screen": "#15202a",
    "pic_frame": "#9cc9e8",
    "pic_taskbar": "#2d4b63",
    "pic_stand": "#4a6b85",
    "pic_window": "#3c3c3c",
    "switch_shown": "#454545",
    "tree": "#5d5d5d",
}
D = DARK

STEP = "step-end"
MOVE = "cubic-bezier(.4,.1,.2,1)"  # the mouse
HIDDEN, SHOWN = "opacity:0", "opacity:1"

# ---- texts ----

# Copies of op-reason's texts. --check looks for each of them in the Rust sources (sources() below).
WINDOW = {  # crates/op-reason-win/src/ui.rs: the table (EN, JA) and the window's own words (WORDS_EN, WORDS_JA)
    "en": {
        "title": "1Password CLI approval request",
        "subtitle": "op-reason · asking 1Password for approval",
        "reason": "Reason",
        "command": "Command",
        # the band's two lines: ⟦…⟧ in the accent color, ⟪…⟫ in the caution color (look.rs: marked)
        "band_title": "1Password's prompt usually opens in the ⟦middle of the screen⟧",
        "band_line": "If it does not, open it from 1Password in the ⟪tray at the bottom right⟫ (under ^)",
        "footer": "Deny any 1Password approval prompt that comes without this window",
    },
    "ja": {
        "title": "1Password CLI の承認の依頼",
        "subtitle": "op-reason · 1Password に承認を求めています",
        "reason": "理由",
        "command": "コマンド",
        "band_title": "1Password の画面は、ふつう⟦画面の中央⟧に出ます",
        "band_line": "出ないときは⟪右下のトレイ⟫の 1Password (^ の中) から",
        "footer": "この窓が出ていない 1Password の承認は拒否してください",
    },
}
SWITCH = (("English", "en"), ("日本語", "ja"))  # ui.rs: SWITCH, the halves of the pill from the left
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


# The SSH relay (crates/op-reason-win/src/relay_ui.rs): its window that asks (per-call) and the toast of a signature
# that passed
RELAY = {
    "en": {
        "title": "SSH key: allow this call?",
        "subtitle": "op-reason's relay · per-call mode",
        "reason": "Reason",
        "claude_pid": "Claude Code (pid {0})",
        "claude": "Claude Code",
        "tree_label": "Process tree",
        "root_mark": "Root",
        "frame_call": "this call",
        "userauth": "logging in as {0}",
        "destination": "Destination",
        "host_key": "host key {0}",
        "key": "Key",
        "countdown": "{0} s left to answer",
        "covers_name": "Allowing covers this {0} until it ends (at most {1} minutes)",
        "band_title": "Allowing brings up 1Password's prompt",
        "band_line": "Usually in the ⟦middle of the screen⟧; if not, from the ⟪tray at the bottom right⟫",
        "allow": "Allow this call",
        "deny": "Deny",
        "pass_word": "Passed",
        "pass_call": "allowed for this call",
        "pass_terminal": "allowed for this terminal",
        "no_reason": "no reason given",
    },
    "ja": {
        "title": "SSH の鍵: この呼び出しを許可しますか",
        "subtitle": "op-reason の中継 · 呼び出しごとに決めるモード",
        "reason": "理由",
        "claude_pid": "Claude Code (pid {0})",
        "claude": "Claude Code",
        "tree_label": "プロセスの木",
        "root_mark": "起点",
        "frame_call": "この呼び出し",
        "userauth": "{0} としてログイン",
        "destination": "接続先",
        "host_key": "ホストの鍵 {0}",
        "key": "鍵",
        "countdown": "答えを待つ残り {0} 秒",
        "covers_name": "許可は、この {0} が終わるまで効きます (長くても {1} 分)",
        "band_title": "許可すると 1Password の画面が出ます",
        "band_line": "ふつうは⟦画面の中央⟧。出ないときは⟪右下のトレイ⟫から",
        "allow": "この呼び出しを許可",
        "deny": "拒否",
        "pass_word": "通過",
        "pass_call": "この呼び出しの許可",
        "pass_terminal": "ターミナルの許可",
        "no_reason": "理由なし",
    },
}
LIMIT_MINUTES, ANSWER_WAIT = 60, 60  # the relay's defaults: an answer lasts an hour at most, a window waits a minute
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
# A card as crates/op-reason-win/src/win/pass_window.rs draws it, in the dark look: the surface with the line's border,
# "Passed", the check, the ×N tag (on the success tint), the bar and the ring in the success color, the caller in the
# text color, line 2 (the command and its destination) in the monospace font, line 3 in the secondary color
TOAST_TEXT_X, TOAST_TEXT_W = 36.0, CARD_W - 36.0 - 12.0
TOAST_LINES = (9.0, 26.0, 43.0)  # the tops of lines 1, 2 and 3; their baselines are 13, 13 and 12 below
TOAST_BODY, TOAST_SMALL = 12.0, 11.0  # line 1 (bold); line 2 (monospace), line 3 and the tags
TOAST_DEST_SHARE = 0.6  # the widest share of line 2 that the destination may take; the command gets the rest
TOAST_SHADOW = 0.4  # the shadow under a card in the dark look
TOAST_MORE_PAD, TOAST_MORE_RADIUS = 6.0, 4.0  # the tag +N: the room left and right of its text, its corners
# The HTML page that plays the relay's scene, and what --check looks for in it
RELAY_HTML = "docs/html/relay-screens.html"


def sources() -> dict:
    """What --check expects to find in each Rust file: the copied texts, formats and sizes (white space aside: see
    squeeze)."""
    ui = ['"Claude Code (pid {pid})"', '"{repo} · {branch}"']
    for t in WINDOW.values():
        ui += [f'"{v}"' for v in t.values()]
    halves = ", ".join(f'("{s}", Lang::{lang.title()})' for s, lang in SWITCH)
    ui.append(f"pub const SWITCH: [(&str, Lang); 2] = [{halves}];")
    ui += [f'"{v}"' for t in NOTIFY.values() for v in t.values()]
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
    sizes = {
        "OUTER_WIDTH": WIN_OUTER,
        "PAD_LEFT": PAD_LEFT,
        "PAD_TOP": PAD_TOP,
        "PAD_RIGHT": PAD_RIGHT,
        "PAD_BOTTOM": PAD_BOTTOM,
        "SCREEN_MARGIN": WIN_TOP,
        "SECTION_GAP": SECTION_GAP,
        "TILE": TILE,
        "TILE_RADIUS": TILE_RADIUS,
        "TILE_GAP": TILE_GAP,
        "TITLE_SIZE": TITLE_SIZE,
        "REASON_SIZE": REASON_SIZE,
        "CODE_SIZE": CODE_SIZE,
        "CHIP_H": CHIP_H,
        "CHIP_PAD": CHIP_PAD,
        "CHIP_MARK": CHIP_MARK,
        "CHIP_GAP": CHIP_GAP,
        "CODE_PAD_X": CODE_PAD_X,
        "CODE_PAD_Y": CODE_PAD_Y,
        "CODE_RADIUS": CODE_RADIUS,
        "MARK_W": MARK_W,
        "BAND_PAD_X": BAND_PAD_X,
        "BAND_PAD_Y": BAND_PAD_Y,
        "BAND_RADIUS": BAND_RADIUS,
        "PILL_H": PILL_H,
        "PILL_PAD": PILL_PAD,
        "PILL_GAP": PILL_GAP,
        "BAR_H": BAR_H,
        "BUTTON_H": BUTTON_H,
        "BUTTON_TOP_GAP": BUTTON_TOP_GAP,
        "BUTTON_GAP": BUTTON_GAP,
        "BUTTON_PAD_X": BUTTON_PAD_X,
        "BUTTON_MIN_W": BUTTON_MIN_W,
        "BUTTON_RADIUS": BUTTON_RADIUS,
        "TREE_INDENT": TREE_INDENT,
        "TREE_LINE": TREE_LINE,
        "TREE_TEXT": TREE_TEXT,
        "TREE_PILL_H": TREE_PILL_H,
        "TREE_INSET": TREE_INSET,
        "TREE_GAP": TREE_GAP,
    }
    window = [f"const {k}: f32 = {v:.1f};" for k, v in sizes.items()] + [
        f"const SKETCH: (f32, f32) = ({SKETCH[0]:.1f}, {SKETCH[1]:.1f});",
        f"const COUNTDOWN_MS: u32 = {COUNTDOWN_MS};",
        'w!("Segoe UI")',
        'w!("Cascadia Mono")',
        # how build() lays the parts out
        "let tw = (right - right_w - 12.0 - tx).max(width / 3.0);",
        "y += TILE.max(title_h + sub_h - 1.0).max(time_h + date_h) + SECTION_GAP;",
        "let (_, _, h) = b.label(&fonts.f10, &v.reason_label, (x0, y), width, Ink::Secondary)?;",
        "y += h + 10.0;",
        "ChipKind::Claude | ChipKind::Person | ChipKind::Other | ChipKind::Repo => CHIP_MARK,",
        "let chip_w = (2.0 * CHIP_PAD + mark + w).min(width);",
        "y = yy - CHIP_GAP + SECTION_GAP;",
        # the relay's process tree under its label (Builder::tree): the rows, the elbows, the box and the small lines
        # under their rows, the frames
        "let label = tree.map_or(&v.code_label, |t| &t.label);",
        "let (_, _, h) = b.label(&fonts.f10, label, (x0, y), width, Ink::Secondary)?;",
        "if let Some(tree) = tree { y += h + 6.0; (y, code_h) = b.tree(tree, y, fit)?;",
        "FactKind::Mono if tree.is_some() => {}",
        "let name_fmt = |solid: bool| if solid { &fonts.f10b } else { &fonts.f10 };",
        "name_w.push(fonts.text(name_fmt(f.solid), &f.label, NO_WRAP)?.1);",
        "let rx = x0 + TREE_INDENT * row.depth as f32; let tx = rx + TREE_TEXT;",
        ".map(|(_, w)| w + 12.0)",
        "let avail = (right - TREE_INSET - room - tx).max(60.0);",
        "RowTone::Strong => (&fonts.f12b, Ink::Text), RowTone::Plain => (&fonts.f12, Ink::Secondary),"
        " RowTone::Faint => (&fonts.f12, Ink::Tertiary),",
        "let (l, w, h) = fonts.text(name_f, &row.name, avail)?;",
        "let (l, w, h) = fonts.text(&fonts.f11, &row.detail, avail)?;"
        " pieces.push(Piece::Text(l, w, h, Ink::Tertiary));",
        "let (l, w, h) = fonts.text(&fonts.f11b, m, avail - 12.0)?;",
        "Piece::Pill(_, w, _, mark) => w + if *mark { 12.0 } else { 14.0 },",
        "Piece::Pill(_, _, h, _) => h.max(TREE_PILL_H),",
        "let at = look::flow(&widths, avail, TREE_GAP);",
        "let mut line_h = vec![TREE_LINE; lines];",
        "line_h[*r] = line_h[*r].max(heights[k]);",
        "let mut yy = top + 2.0;",
        "let mid = top + 2.0 + line_h[0] / 2.0;",
        "(0..i).rev().find(|&k| tree.rows[k].depth + 1 == row.depth)",
        "let px = x0 + TREE_INDENT * tree.rows[p].depth as f32 + 5.0;",
        "self.line((px, mids[p] + 6.0), (px, mid), Ink::Tree, 1.5);",
        "self.line((px, mid), (tx - 4.0, mid), Ink::Tree, 1.5);",
        "Ink::Tree => p.tree,",
        "ChipKind::Claude => Ink::Claude, ChipKind::Person => Ink::Success, _ => Ink::Secondary,",
        "cx: rx + 5.0, cy: mid, r: 5.0,",
        "y: ly + (lh - h) / 2.0,",
        "let (pad, fill, stroke, ink) = if mark { (6.0, None, Some((Ink::Caution, 1.0)), Ink::Caution) } else {",
        "let py = ly + (lh - ph) / 2.0;",
        "self.rect(rect(x, py, widths[k], ph), ph / 2.0, fill, stroke);",
        "x: x + pad, y: py + (ph - h) / 2.0,",
        "y = yy + 2.0; let inner = right - TREE_INSET - tx; if i == tree.boxed { y += 2.0;"
        " let (box_h, h) = self.code_box(&tree.code, (tx, y), inner, fit)?;",
        "if let Some(note) = &tree.note { y += 3.0;"
        " let (_, _, h) = self.label(&fonts.m10, note, (tx, y), inner, Ink::Tertiary)?; y += h; } y += 4.0; }",
        "let (_, _, h) = self.label(&fonts.m10, line, (tx, y), inner, Ink::Tertiary)?; y += h + 4.0;",
        "let left = x0 + TREE_INDENT * first.depth as f32 - 5.0;",
        "let fr = right - 8.0 * k as f32;",
        "r: Rect { left, top: top - 1.0, right: fr, bottom: end + 1.0 + 3.0 * (n - 1 - k) as f32, }, radius: 6.0,"
        " fill: None, stroke: Some((Ink::Accent, if f.solid { 1.5 } else { 1.0 })), dashed: !f.solid,",
        "(0.0, top + 3.0),",
        "self.labels[i].x = fr - 6.0 - w;",
        "let box_h = code_h + 2.0 * CODE_PAD_Y;",
        "let (_, _, h) = b.label(&fonts.m10, note, (x0, y), width, Ink::Tertiary)?;",
        "let (_, _, h) = b.label(&fonts.m10, &fact.text, at, w, Ink::Tertiary)?;",
        "y += BAR_H + 7.0;",
        "let w = width - pill_w - PILL_GAP;",
        'format!(" · {}", v.status)',
        "y += (h + 5.0).max(PILL_H);",
        "let bx = x0 + BAND_PAD_X + SKETCH.0 + 12.0;",
        "let inner = th + 3.0 + lh;",
        "let bh = inner.max(SKETCH.1) + 2.0 * BAND_PAD_Y;",
        "b.pill_width(lang)? + PILL_GAP",
        "width - MARK_W - pill_w,",
        "let row = if pill_here { h.max(PILL_H) } else { h };",
        "pill = Some(b.pill(lang, right, y + row - PILL_H)?);",
        "let bw = (w + 2.0 * BUTTON_PAD_X).max(BUTTON_MIN_W);",
        # the picture in the band
        "rect(x, y, SKETCH.0, 40.0),",
        "rect(x + 44.0, y + 4.0, 16.0, 9.0),",
        "let prompt = rect(x + 22.0, y + 12.0, 20.0, 13.0);",
        "Sketch::Op => (Some((Ink::Accent, 1.0)), None, false),",
        "Sketch::Ask => (Some((Ink::Accent, 0.35)), Some((Ink::Accent, 1.0)), true),",
        "rect(x + 1.0, y + 34.0, 62.0, 5.0),",
        "rect(x + 28.0, y + 40.0, 8.0, 4.0),",
        "rect(x + 22.0, y + 44.0, 20.0, 2.0),",
        # the switch's shown half, and the buttons' colors by their place: the first (unless it is Deny) filled, its
        # label bold; Deny outlined in the critical color; any other outlined in the accent color
        "ink: if l == lang { Ink::Text } else { Ink::Secondary },",
        "&rounded(inset(pill.rects[shown], 2.0), PILL_H / 2.0 - 2.0),",
        "let filled = n == 0 && *button != Button::Deny; let fmt = if filled { &fonts.f12b } else { &fonts.f12 };",
        "let (fill, border) = if b.filled { (Ink::Accent, None) } else if b.button == Button::Deny {"
        " (Ink::Surface, Some(Ink::Critical)) } else { (Ink::Surface, Some(Ink::Accent)) };",
        "let ink = if filled { Ink::OnAccent } else if *button == Button::Deny { Ink::Critical } else { Ink::Accent };",
    ]
    settings = [
        SETTINGS[0],
        'pub const SETTINGS_FILE: &str = "settings.json";',
        "pub const VERSION: u64 = 1;",
        'assert_eq!(render(None, Lang::Ja), r#"{"v":1,"lang":"ja"}"#);',
    ]
    values = [
        f"pub const DEFAULT_TIME_LIMIT: u64 = {LIMIT_MINUTES} * 60;",
        f"pub const DEFAULT_ANSWER_WAIT: u64 = {ANSWER_WAIT};",
    ]
    relay_ui = [f'"{v}"' for t in RELAY.values() for v in t.values()] + [
        'format!("{} {d}", t.destination)',
        '(Some(d), Some(k)) => Some(format!("{d} · {}", fill(t.host_key, k))),',
        'format!("{} {k}", t.key)',
        'text: detail.join(" · "),',
        # a call with a root and its processes: the tree (tree_view) from Claude Code (its dot, its image and PID) to
        # the root (its mark) and the client; the command the root runs in the box under the root's row and the rest of
        # its line under the box; the client's command line small under its row (the window leaves its line among the
        # facts out when it draws the tree); the frame
        # of what Allow this call covers, from the root down, while the window asks; and the program in the line about
        # how long an answer lasts
        "match look::root_command(&root.cmd) { Some(u) => (root_at, u.command, Some(u.frame)),",
        "if f.root.is_some() && !powershell {",
        "let frames = phase == Phase::Ask && !alone;",
        'let pid_of = |p: Option<u32>| p.map(|n| format!("pid {n}")).unwrap_or_default();',
        "Caller::Claude { .. } => ChipKind::Claude,",
        "Caller::Claude { .. } => t.claude,",
        "let detail = [h.name.clone(), pid_of(h.pid)]",
        '.join(" · ");',
        "dot: Some(dot), ..plain(0, name, detail, RowTone::Strong)",
        "mark: Some(t.root_mark.to_owned()), ..plain(root_at, &root.name, pid_of(Some(root.pid)), RowTone::Strong)",
        "rows.push(plain(rows.len(), &c.name, pid_of(c.pid), RowTone::Strong));",
        ".then(|| (client_at.unwrap_or(root_at), f.command.clone()));",
        "out.push(frame(root_at, t.frame_call, true));",
        "let named = f.root.is_some() && !powershell;",
        "match named.then(|| look::program_name(&code)).flatten() {",
        'Some(name) => t.covers_name.replace("{0}", &name).replace("{1}", &limit),',
        'format!("{covers}{waiting}"),',
    ]
    look = [
        'frame: format!("{head}\\"…\\""),',
        "let kind = if i == 0 || (i == 1 && plain_word(&t.value)) {",
        "w.starts_with(['/', '~', '.']) || w.starts_with(\"op://\") || w.contains('\\\\')",
        "Some(sub) if name == \"git\" && plain_word(&sub.value) => {",
    ]
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
        f"const BODY_SIZE: f32 = {TOAST_BODY:.1f};",
        f"const SMALL_SIZE: f32 = {TOAST_SMALL:.1f};",
        "const BODY_LINE: (f32, f32) = (17.0, 13.0);",
        "const SMALL_LINE: (f32, f32) = (16.0, 12.0);",
        "const CHECK: (f32, f32, f32) = (18.0, 17.5, 8.0);",
        "const RADIUS: f32 = 6.0;",
        "const BAR_TOP: f32 = CARD_H - 5.0;",
        f"const DEST_SHARE: f32 = {TOAST_DEST_SHARE};",
        f"const MORE_PAD: f32 = {TOAST_MORE_PAD:.1f};",
        f"const MORE_RADIUS: f32 = {TOAST_MORE_RADIUS:.1f};",
        f"const SHADOW: (f32, f32) = (0.16, {TOAST_SHADOW});",
        # the roles of the palette that a card takes, and line 2 in the windows' monospace font
        "fill: brush(p.surface, 1.0)?,",
        "border: brush(p.line, 1.0)?,",
        "glow: brush(p.success, 0.9)?,",
        "green: brush(p.success, 1.0)?,",
        "pale: brush(p.success_tint, 1.0)?,",
        "white: brush(p.on_success, 1.0)?,",
        "black: brush(p.text, 1.0)?,",
        "dim: brush(p.secondary, 1.0)?,",
        "window::mono_family(&dw),",
        '&format!(" → {d}"),',
        "TEXT_W * DEST_SHARE,",
        "rt.DrawTextLayout(at(TEXT_X, LINE2_TOP), &l.command.layout, &p.black, options);",
        "rt.DrawTextLayout(at(TEXT_X, LINE3_TOP), &l.why.layout, &p.dim, options);",
    ]
    return {
        "crates/op-reason-win/src/ui.rs": ui,
        "crates/op-reason-win/src/relay_ui.rs": relay_ui,
        "crates/op-reason-win/src/pass_toast.rs": pass_toast,
        "crates/op-reason-win/src/win/pass_window.rs": pass_window,
        "crates/op-reason-win/src/win/window.rs": window,
        "crates/op-reason-win/src/look.rs": look,
        "crates/op-reason-core/src/messages.rs": messages,
        "crates/op-reason-core/src/logview.rs": logview,
        "crates/op-reason-core/src/settings/file.rs": settings,
        "crates/op-reason-core/src/settings/values.rs": values,
    }


THEME = "crates/op-reason-win/src/theme.rs"


def dark_needles() -> list:
    """What --check expects in the DARK palette of theme.rs: each color the animations draw, by its role."""
    return [f"    {role}: Rgb::hex(0x{rgb[1:]})," for role, rgb in DARK.items()]


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
REPO, BRANCH = "example/infra", "main"  # the place of the calls: this repository and branch, in PROMPT_PATH
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
            'staging VM" read "op://Private/proxmox/token". op-reason\'s window opens at the top right of the screen, '
            "dark: the reason first, in large letters; the caller (Claude Code and its process id), the repository "
            "with its branch and the directory as chips; the command in a monospace box, the program and the reference "
            "in color; a band that marks where 1Password's prompt usually opens. The 1Password approval prompt (drawn "
            "as a plain box) opens in the middle of the screen. After a click on Approve, op prints the value (hidden "
            "here), op-reason signs out and its window closes, and op-reason-log shows the call as one line: approved, "
            "by Claude, with the reason and the command."
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
            "Animation: op-reason's window opens in English, its default, at the top right of the screen, with the "
            "language switch, a pill of English and 日本語, at the right of its last row, and the 1Password approval "
            "prompt (drawn as a plain box) opens in the middle of the screen. A click on 日本語 turns the window "
            "Japanese at once: 日本語 is now bold on the lighter half of the pill, and English is the half to click. "
            "The prompt keeps the focus, and the choice is kept in "
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
            "Animation of a Windows desktop. A signature that passes without a window shows a small dark card in the "
            "colors of op-reason's window, a toast, at the top left, at 90% opacity so that the text behind shows "
            "through faintly: Passed · "
            "Human, the ssh command and its destination, allowed for this terminal · no reason given; it fades after "
            "four seconds. An ansible run signs eight times in one call: one card counts them (×2 … ×8), shows the last "
            "host and lights up at each. Three other calls each put a card on top and push the others down; a fourth "
            "and a fifth push the oldest out at once, and +N under the third card counts the cards pushed out (+1, "
            "+2) until four seconds after their last signature; four seconds after its last signature each card "
            "fades. Then a new call asks in op-reason's window at the top right. Its process tree goes from Claude Code "
            "to bash.exe, marked Root, with the command it runs in a box under it, then to ssh.exe with its command "
            "line; a frame named this call surrounds the root and below: what Allow this call covers. The seconds left "
            "to answer count down, with the window's Windows notification at the bottom right, while another toast "
            "comes at the top left; a click on Allow this call closes the "
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
            "を打つ。画面の右上に op-reason の窓が暗い色で開き、一番上に理由を大きな字で、呼び手 (Claude Code と "
            "process id)・repository と branch・ディレクトリをチップで、コマンドを等幅の枠の中に色分けして出し、"
            "1Password の画面がふつう出る所を帯で示す。画面の真ん中に 1Password の承認の画面 (素の箱で描いたもの) "
            "が出る。許可を押すと "
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
            "アニメーション: 画面の右上に op-reason の窓が既定の英語で開き、最下行の右に言語の切り替え (English と "
            "日本語 を並べた丸いスイッチ) が出る。画面の真ん中には 1Password の承認の画面 (素の箱で描いたもの) が出る。"
            "日本語 を押すと、窓はその場で日本語に変わる (日本語 が明るい側で太字になり、English が押せる側になる)。"
            "フォーカスは承認の画面のままで、"
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
            "アニメーション。Windows の画面で、窓を出さずに通った署名は、左上に op-reason の窓と同じ暗い色の小さな"
            "カード (トースト) を"
            "後ろの字がうっすら透ける濃さ 90% で出す: 通過 · 人、ssh のコマンドと接続先、ターミナルの許可 · 理由なし。4 秒で"
            "薄れて消える。ansible の 1 回の実行は同じ呼び出しで 8 回署名し、1 枚のカードが数 (×2 … ×8) と最後の"
            "ホストを出し、署名のたびに光る。ほかの 3 つの呼び出しはそれぞれ一番上にカードを入れてほかを下へずらし、"
            "4 枚目と 5 枚目は一番古いものをすぐ消す。消したカードは、その最後の署名から 4 秒まで 3 枚目の下の +N が"
            "数える (+1、+2)。どのカードも最後の署名から 4 秒で薄れて消える。最後に、新しい呼び出しが右上の op-reason "
            "の窓で聞く。窓のプロセスの木は Claude Code から、起点の印の付いた bash.exe (動かすコマンドをその下の箱に"
            "出す) を経て ssh.exe (その下にコマンドを出す) まで続き、起点から下を「この呼び出し」の枠が囲む (この呼び"
            "出しを許可 が効く範囲)。答えを待つ残り秒が減っていき、右下に Windows "
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


def wide(ch: str) -> bool:
    u = ord(ch)
    return any(a <= u <= b for a, b in WIDE)


# A Japanese line starts with none of these and ends with none of those (kinsoku)
NO_LINE_START = set("、。，．・：；？！ー)）]」』〕｝〉》】…ぁぃぅぇぉっゃゅょァィゥェォッャュョ")
NO_LINE_END = set("(（[「『〔｛〈《【")


def wrap_at(s: str, fits) -> list:
    """The lines of `s` as (start, end) without the spaces at their ends, broken as DirectWrite breaks them: after
    spaces and between full-width characters, each line as long as `fits(line)` allows; a word too long for a line is
    cut where it overflows."""
    pieces, start = [], 0  # what a line breaks between: a word and the spaces after it, or one full-width character
    for i in range(1, len(s)):
        a, b = s[i - 1], s[i]
        if b != " " and (
            a == " " or ((wide(a) or wide(b)) and a not in NO_LINE_END and b not in NO_LINE_START)
        ):
            pieces.append((start, i))
            start = i
    pieces.append((start, len(s)))
    lines: list = []
    cur = None
    for a, b in pieces:
        if cur is None:
            cur = (a, b)
        elif fits(s[cur[0] : b].rstrip(" ")):
            cur = (cur[0], b)
        else:
            lines.append(cur)
            cur = (a, b)
        while cur[1] - cur[0] > 1 and not fits(s[cur[0] : cur[1]].rstrip(" ")):
            k = cur[1] - 1
            while k - cur[0] > 1 and not fits(s[cur[0] : k]):
                k -= 1
            lines.append((cur[0], k))
            cur = (k, cur[1])
    lines.append(cur)
    out = []
    for a, b in lines:
        while b > a and s[b - 1] == " ":
            b -= 1
        out.append((a, b))
    return out


def unmark(s: str) -> tuple:
    """look.rs: marked — the text of a band's line without its marks, and the tone of each character: "" (plain),
    "accent" (⟦…⟧) or "caution" (⟪…⟫). A mark that does not close is a plain character."""
    out, tones, i = [], [], 0
    while i < len(s):
        close = {"⟦": "⟧", "⟪": "⟫"}.get(s[i])
        end = s.find(close, i + 1) if close else -1
        if end >= 0:
            out.append(s[i + 1 : end])
            tones += ["accent" if s[i] == "⟦" else "caution"] * (end - i - 1)
            i = end + 1
        else:
            out.append(s[i])
            tones.append("")
            i += 1
    return "".join(out), tones


def lh(size: float) -> float:
    """The height of a line of Segoe UI at `size`."""
    return size * SEGOE_LINE


def wline(x: float, top: float, s: str, size: float, fill: str, bold: bool = False, anchor: str = "", tones=()) -> str:
    """One line of op-reason's window with its top at `top` (DirectWrite lays lines out from their tops), in the room
    est_width gives it, so that every viewer's font keeps the window's layout. `tones`: the tone of each character
    (unmark): each run of one tone is a text of its own, in the accent or the caution color, where the line's text
    before it ends (one text with tspans would not keep its room in every renderer)."""
    if not s:
        return ""
    if any(tones):
        tint = {"": fill, "accent": D["accent"], "caution": D["caution"]}
        out, at = [], x
        for t, run in itertools.groupby(zip(s, tones, strict=True), key=lambda p: p[1]):
            part = "".join(ch for ch, _ in run)
            out.append(wline(at, top, part, size, tint[t], bold))
            at += est_width(part, size, bold)
        return "".join(out)
    a = f' text-anchor="{anchor}"' if anchor else ""
    c = ' class="b"' if bold else ""
    return (
        f'<text x="{num(x)}" y="{num(top + size * SEGOE_ASCENT)}" font-size="{num(size)}" fill="{fill}"{c}{a} '
        f'textLength="{num(est_width(s, size, bold))}" lengthAdjust="spacingAndGlyphs" xml:space="preserve">'
        f"{esc(s)}</text>"
    )


def para(x: float, top: float, s: str, size: float, fill: str, width: float, bold: bool = False) -> tuple:
    """A text of op-reason's window wrapped at `width`, its words in ⟦…⟧ and ⟪…⟫ tinted: the lines, the height and
    the widest line's width."""
    plain, tones = unmark(s)
    spans = wrap_at(plain, lambda t: est_width(t, size, bold) <= width + 1e-6)
    out = [
        wline(x, top + i * lh(size), plain[a:b], size, fill, bold, tones=tones[a:b]) for i, (a, b) in enumerate(spans)
    ]
    return "".join(out), len(spans) * lh(size), max(est_width(plain[a:b], size, bold) for a, b in spans)


def mono_para(x: float, top: float, s: str, size: float, fill: str, width: float) -> tuple:
    """A small text of op-reason's window in the monospace font (the root's line under the command's box, the ssh
    command among the facts or under the client's row of the process tree), wrapped at `width`: the lines and the
    height."""
    n = int(width // (size * ADVANCE))
    spans = wrap_at(s, lambda t: cols(t) <= n)
    line = size * MONO_LINE
    out = [mono(x, top + i * line + size * MONO_ASCENT, s[a:b], size, fill) for i, (a, b) in enumerate(spans)]
    return "".join(out), len(spans) * line


def box(x: float, y: float, w: float, h: float, rx: float, fill: str, stroke: str = "", sw: float = 1, more="") -> str:
    """A rectangle with round corners (Direct2D's rounded rectangle: the stroke on its edge)."""
    r = f' rx="{num(min(rx, w / 2, h / 2))}"' if rx else ""
    s = f' stroke="{stroke}"' + (f' stroke-width="{num(sw)}"' if sw != 1 else "") if stroke else ""
    return f'<rect x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{num(h)}"{r} fill="{fill}"{s}{more}/>'


def dot(cx: float, cy: float, r: float, fill: str) -> str:
    return f'<circle cx="{num(cx)}" cy="{num(cy)}" r="{num(r)}" fill="{fill}"/>'


def command_tokens(s: str):
    """look.rs: tokens — the words of a command line as op-reason displays it, (value, start, end) each: words between
    spaces, a word in double quotes holding spaces (\\" a quote inside); None when a quote does not close."""
    out, i, n = [], 0, len(s)
    while i < n:
        if s[i] in " \t":
            i += 1
            continue
        start, value, quoted, end = i, [], False, n
        while i < n:
            c = s[i]
            if not quoted and c in " \t":
                end = i
                break
            i += 1
            if c == '"':
                quoted = not quoted
            elif c == "\\" and quoted and i < n and s[i] == '"':
                i += 1
                value.append('"')
            else:
                value.append(c)
        if quoted:
            return None
        out.append(("".join(value), start, end))
    return out


def plain_word(w: str) -> bool:
    """look.rs: plain_word — letters, digits, - and _, starting with a letter (a subcommand such as push)."""
    return bool(w) and w[0].isalpha() and all(c.isalnum() or c in "-_" for c in w)


def program_name(command: str) -> str:
    """look.rs: program_name — the program of a command of one program (its file name without .exe), and for git its
    subcommand: the name in the relay's line about how long an answer lasts."""
    words = command_tokens(command) or [("", 0, 0)]
    first = words[0][0].replace("\\", "/").rsplit("/", 1)[-1]
    shown = first[:-4] if first.lower().endswith(".exe") else first
    if shown.lower() == "git" and len(words) > 1 and plain_word(words[1][0]):
        return f"{shown} {words[1][0]}"
    return shown


def color_words(command: str) -> list:
    """look.rs: color_words (design §4.6) — (start, end, kind) of each word: "program" for the program and, when the
    next word is a plain word, that word too (the accent color, bold); "path" for a file name, a path or a reference
    (the path color); "plain" for the rest. The spaces between words are in none."""
    words = command_tokens(command)
    if words is None:
        return [(0, len(command), "plain")] if command else []

    def path_word(w: str) -> bool:
        if w.startswith(("/", "~", ".", "op://")) or "\\" in w:
            return True
        return any(w[k] == "." and w[k - 1].isalnum() and w[k + 1].isalnum() for k in range(1, len(w) - 1))

    out: list = []
    for i, (value, start, end) in enumerate(words):
        program = i == 0 or (i == 1 and plain_word(value))
        if i == 1 and program:
            out[-1] = (out[-1][0], end, "program")
        else:
            out.append((start, end, "program" if program else "path" if path_word(value) else "plain"))
    return out


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


# ---- op-reason's window (window.rs), in the dark look ----


def tile(icon: str, x: float, y: float) -> str:
    """The header's tile (window.rs: tile): a lock on the accent color (a call of op), a key on the caution color (the
    relay)."""
    if icon == "lock":
        ink = D["on_accent"]
        return (
            box(x, y, TILE, TILE, TILE_RADIUS, D["accent"])
            + box(x + 9.5, y + 7, 9, 10, 4.5, "none", ink, 2)
            + box(x + 8, y + 13, 12, 9, 2, ink)
        )
    ink = D["on_caution"]
    return (
        box(x, y, TILE, TILE, TILE_RADIUS, D["caution"])
        + f'<circle cx="{num(x + 10)}" cy="{num(y + 14)}" r="4" fill="none" stroke="{ink}" stroke-width="2"/>'
        f'<path d="M{num(x + 14)},{num(y + 14)}H{num(x + 22)}M{num(x + 19)},{num(y + 14)}V{num(y + 18)}'
        f'M{num(x + 22)},{num(y + 14)}V{num(y + 17)}" fill="none" stroke="{ink}" stroke-width="2"/>'
    )


def shield(x: float, y: float) -> str:
    """The footer's shield (12 × 14)."""
    return (
        f'<path d="M{num(x + 6)},{num(y)}L{num(x + 12)},{num(y + 2.5)}V{num(y + 7)}L{num(x + 6)},{num(y + 14)}'
        f'L{num(x)},{num(y + 7)}V{num(y + 2.5)}Z" fill="none" stroke="{D["critical"]}" stroke-width="1.5"/>'
    )


def purpose_mark(x: float, y: float) -> str:
    """The mark of what the signature is for: an arrow into a circle."""
    cx, cy, ink = x + 7, y + 8, D["secondary"]
    return (
        f'<circle cx="{num(cx)}" cy="{num(cy)}" r="6" fill="none" stroke="{ink}" stroke-width="1.5"/>'
        f'<path d="M{num(cx - 3)},{num(cy)}H{num(cx + 3)}M{num(cx + 1)},{num(cy - 2)}L{num(cx + 3)},{num(cy)}'
        f'L{num(cx + 1)},{num(cy + 2)}" fill="none" stroke="{ink}" stroke-width="1.2"/>'
    )


def sketch(kind: str, x: float, y: float) -> str:
    """The band's picture of the screen (window.rs: sketch, design §4.9): op-reason's window at the top right (in the
    caution color: the window that asks), the place of 1Password's prompt in the middle (filled for a call of op;
    dashed and pale for the relay's window that asks: to come), the tray's dot at the bottom right of the taskbar."""
    if kind == "op":
        prompt = box(x + 22, y + 12, 20, 13, 2, D["accent"])
    else:
        prompt = box(x + 22, y + 12, 20, 13, 2, D["accent"], more=' fill-opacity=".35"') + box(
            x + 22, y + 12, 20, 13, 2, "none", D["accent"], more=' stroke-dasharray="2 2"'
        )
    return (
        box(x, y, SKETCH[0], 40, 3, D["pic_screen"], D["pic_frame"], 1.2)
        + box(x + 44, y + 4, 16, 9, 1.5, D["caution"] if kind == "ask" else D["pic_window"], D["secondary"], 0.8)
        + prompt
        + box(x + 1, y + 34, 62, 5, 0, D["pic_taskbar"])
        + dot(x + 57, y + 36.5, 2, D["caution"])
        + box(x + 28, y + 40, 8, 4, 0, D["pic_stand"])
        + box(x + 22, y + 44, 20, 2, 0, D["pic_stand"])
    )


def pill(lang: str, right: float, top: float) -> tuple:
    """The language switch (window.rs: pill; look.rs: pill): English then 日本語, each its name's width and PILL_PAD on
    both sides, ending at `right`; the language shown bold in the text color on the lighter part, the other one in the
    secondary color, the half a click switches to. Returns the parts and the halves (left, right)."""
    widths = [est_width(s, 11, code == lang) for s, code in SWITCH]
    ja, en = widths[1] + 2 * PILL_PAD, widths[0] + 2 * PILL_PAD
    halves = ((right - ja - en, right - ja), (right - ja, right))
    left = halves[0][0]
    sl, sr = halves[[code for _, code in SWITCH].index(lang)]
    out = [
        box(left, top, right - left, PILL_H, PILL_H / 2, D["surface"]),
        box(left + 0.5, top + 0.5, right - left - 1, PILL_H - 1, PILL_H / 2, "none", D["line"]),
        box(sl + 2, top + 2, sr - sl - 4, PILL_H - 4, PILL_H / 2 - 2, D["switch_shown"]),
    ]
    for (name, code), (hl, _) in zip(SWITCH, halves, strict=True):
        shown = code == lang
        ink = D["text"] if shown else D["secondary"]
        out.append(wline(hl + PILL_PAD, top + (PILL_H - lh(11)) / 2, name, 11, ink, shown))
    return "".join(out), halves


def flow(widths: list, row: float, gap: float) -> list:
    """look.rs: flow — the row and the x of each item, `gap` apart; an item wider than a row takes a row of its own."""
    out, r, x = [], 0, 0.0
    for w in widths:
        if x > 0 and x + w > row:
            r, x = r + 1, 0.0
        out.append((r, x))
        x += w + gap
    return out


def code_text(x: float, top: float, command: str, width: float) -> tuple:
    """The command in its monospace box (design §4.6): its words colored (the program bold in the accent color, paths in
    the path color, the rest in the code's text color), wrapped at spaces to `width`. Returns the lines and the
    height."""
    kinds = ["plain"] * len(command)
    for a, b, kind in color_words(command):
        kinds[a:b] = [kind] * (b - a)
    look = {"program": (D["accent"], True), "path": (D["code_path"], False), "plain": (D["code_text"], False)}
    n = int(width // (CODE_SIZE * ADVANCE))
    spans = wrap_at(command, lambda t: cols(t) <= n)
    out = []
    for i, (a, b) in enumerate(spans):
        parts = []  # runs of one kind
        for kind, run in itertools.groupby(zip(command[a:b], kinds[a:b], strict=True), key=lambda p: p[1]):
            parts.append(("".join(ch for ch, _ in run), *look[kind]))
        out.append(runs(x, top + i * CODE_SIZE * MONO_LINE + CODE_SIZE * MONO_ASCENT, parts, CODE_SIZE))
    return "".join(out), len(spans) * CODE_SIZE * MONO_LINE


class Laid:
    """A window laid out (window.rs: build) in DIPs of its client area, drawn in Direct2D's order: the shapes, the
    texts, then the countdown, the switch and the buttons. `at`: where the scenes point (the caller's chip, the reason,
    the switch's halves, the buttons, the countdown)."""

    def __init__(self) -> None:
        self.shapes: list = []
        self.texts: list = []
        self.top: list = []
        self.h = 0.0
        self.at: dict = {}

    def svg(self, countdown: str = "") -> str:
        return "".join(self.shapes + self.texts) + countdown + "".join(self.top)


def lay_out_tree(w: Laid, v: dict, y: float) -> float:
    """The relay's process tree from `y`, as window.rs's Builder::tree lays it out (design
    2026-11-27-relay-process-tree-design.md §3): a row per process, each child indented under its parent with an elbow
    in the tree color; a row's parts flow after its dot (the caller's), from its text's start: the name, the detail in
    the tertiary color, the mark outlined in the caution color (the root's); the box (the view's code, its code_note
    small under it) under its row, and the small line (the client's command line) under its row; then the frames in the
    gaps, each with its name at its top right. Returns where the tree ends."""
    tree = v["tree"]
    rows, frames = tree["rows"], tree["frames"]
    x0, right = PAD_LEFT, PAD_LEFT + TEXT_W
    tones = {"strong": (True, D["text"]), "plain": (False, D["secondary"]), "faint": (False, D["tertiary"])}
    name_w = [est_width(f["label"], 10, f["solid"]) for f in frames]
    tops, mids, ends = [], [], []
    for i, row in enumerate(rows):
        rx = x0 + TREE_INDENT * row["depth"]
        tx = rx + TREE_TEXT
        # the parts flow onto more lines rather than run under the end, or under a frame's name on its first row
        room = max((nw + 12 for f, nw in zip(frames, name_w, strict=True) if f["first"] == i), default=0.0)
        avail = max(right - TREE_INSET - room - tx, 60.0)
        bold, ink = tones[row["tone"]]
        parts = [(row["name"], 12, bold, ink, False)]  # (text, size, bold, ink, in a pill: the mark)
        if row["detail"]:
            parts.append((row["detail"], 11, False, D["tertiary"], False))
        if row.get("mark"):
            parts.append((row["mark"], 11, True, D["caution"], True))
        sized = []  # each part's width, height and its text's height
        for s, size, b, _, pill in parts:
            _, h, tw = para(0, 0, s, size, "", avail - 12 if pill else avail, b)
            sized.append((tw + 12, max(h, TREE_PILL_H), h) if pill else (tw, h, h))
        at = flow([p[0] for p in sized], avail, TREE_GAP)
        line_h = [TREE_LINE] * (at[-1][0] + 1)
        for (r, _), (_, ph, _) in zip(at, sized, strict=True):
            line_h[r] = max(line_h[r], ph)
        top = y
        line_y, yy = [], top + 2
        for h in line_h:
            line_y.append(yy)
            yy += h
        mid = top + 2 + line_h[0] / 2
        # the elbow from the parent: down from below its dot or name, then right to this row (two lines, flat ends)
        parent = next((k for k in range(i - 1, -1, -1) if rows[k]["depth"] + 1 == row["depth"]), None)
        if row["depth"] > 0 and parent is not None:
            px = x0 + TREE_INDENT * rows[parent]["depth"] + 5
            w.shapes.append(
                f'<path d="M{num(px)},{num(mids[parent] + 6)}V{num(mid)}M{num(px)},{num(mid)}H{num(tx - 4)}" '
                f'fill="none" stroke="{D["tree"]}" stroke-width="1.5"/>'
            )
        if row.get("dot"):
            dot_ink = {"claude": D["claude"], "person": D["success"]}.get(row["dot"], D["secondary"])
            w.shapes.append(dot(rx + 5, mid, 5, dot_ink))
        for (s, size, b, ink, pill), (pw, ph, h), (r, px) in zip(parts, sized, at, strict=True):
            x, ly, line = tx + px, line_y[r], line_h[r]
            if pill:
                py = ly + (line - ph) / 2
                w.shapes.append(box(x, py, pw, ph, ph / 2, "none", D["caution"]))
                w.texts.append(para(x + 6, py + (ph - h) / 2, s, size, ink, avail - 12, b)[0])
            else:
                w.texts.append(para(x, ly + (line - h) / 2, s, size, ink, avail, b)[0])
        y = yy + 2
        inner = right - TREE_INSET - tx
        if i == tree["boxed"]:  # the box, and under it the rest of the root's line
            y += 2
            code, code_h = code_text(tx + CODE_PAD_X, y + CODE_PAD_Y, v["code"], inner - 2 * CODE_PAD_X)
            box_h = code_h + 2 * CODE_PAD_Y
            w.shapes.append(box(tx, y, inner, box_h, CODE_RADIUS, D["code_bg"], D["code_border"]))
            w.texts.append(code)
            y += box_h
            if v["code_note"]:
                y += 3
                note, h = mono_para(tx, y, v["code_note"], 10, D["tertiary"], inner)
                w.texts.append(note)
                y += h
            y += 4
        if tree["small"] and tree["small"][0] == i:
            small, h = mono_para(tx, y, tree["small"][1], 10, D["tertiary"], inner)
            w.texts.append(small)
            y += h + 4
        tops.append(top)
        mids.append(mid)
        ends.append(y)
    # the frames, the outer one first: each inner one ends 8 DIPs inside the one around it; solid for the filled
    # button's (its name bold), dashed for the outlined one's
    for k, (f, nw) in enumerate(zip(frames, name_w, strict=True)):
        top, fr = tops[f["first"]], right - 8 * k
        left = x0 + TREE_INDENT * rows[f["first"]]["depth"] - 5
        bottom = ends[f["last"]] + 1 + 3 * (len(frames) - 1 - k)
        sw, dashed = (1.5, "") if f["solid"] else (1, ' stroke-dasharray="2 2"')
        w.shapes.append(box(left, top - 1, fr - left, bottom - top + 1, 6, "none", D["accent"], sw, dashed))
        w.texts.append(wline(fr - 6 - nw, top + 3, f["label"], 10, D["accent"], f["solid"]))
    return y


def lay_out(v: dict, lang: str) -> Laid:
    """Lays a window's view out top to bottom as window.rs's build does (design §4.2, §4.3): the header, the reason,
    the chips, the command (the relay's process tree with the box under its row), what the signature is for (the relay),
    the countdown with the switch (the relay's window that asks), the band, the footer with the switch (a call of op),
    the buttons (the relay)."""
    w = Laid()
    x0, width = PAD_LEFT, TEXT_W
    right = x0 + width
    y = PAD_TOP

    # the header: the tile, the title and the subtitle, the time and the date at the right
    w.shapes.append(tile(v["icon"], x0, y))
    w.texts.append(wline(right, y, v["time"], 12, D["secondary"], anchor="end"))
    right_w, date_h = est_width(v["time"], 12), 0.0
    if v["date"]:
        w.texts.append(wline(right, y + lh(12), v["date"], 10, D["tertiary"], anchor="end"))
        right_w, date_h = max(right_w, est_width(v["date"], 10)), lh(10)
    tx = x0 + TILE + TILE_GAP
    tw = max(right - right_w - 12 - tx, width / 3)
    title, title_h, _ = para(tx, y - 1, v["title"], TITLE_SIZE, D["text"], tw, bold=True)
    sub, sub_h, _ = para(tx, y - 1 + title_h, v["subtitle"], 11, D["secondary"], tw)
    w.texts += [title, sub]
    y += max(TILE, title_h + sub_h - 1, lh(12) + date_h) + SECTION_GAP

    # the reason, the largest text
    label, h, _ = para(x0, y, v["reason_label"], 10, D["secondary"], width)
    w.texts.append(label)
    y += h + 2
    reason, h, rw = para(x0, y, v["reason"], REASON_SIZE, D["text"], width, bold=True)
    w.texts.append(reason)
    w.at["reason"] = (x0, y, rw, h)
    y += h + 10

    # the chips, in rows: the caller with its dot, the repository with its mark, the directory
    chips = []
    for kind, s in v["chips"]:
        mark = CHIP_MARK if kind in ("claude", "person", "other", "repo") else 0.0
        _, text_h, text_w = para(0, 0, s, 11, D["text"], width - 2 * CHIP_PAD - mark)
        chips.append((kind, s, min(2 * CHIP_PAD + mark + text_w, width), max(text_h + 8, CHIP_H), text_h, mark))
    if chips:
        at = flow([c[2] for c in chips], width, CHIP_GAP)
        row_h = [0.0] * (at[-1][0] + 1)
        for (r, _), c in zip(at, chips, strict=True):
            row_h[r] = max(row_h[r], c[3])
        row_y, yy = [], y
        for rh in row_h:
            row_y.append(yy)
            yy += rh + CHIP_GAP
        for i, ((kind, s, cw, ch, text_h, mark), (r, cx)) in enumerate(zip(chips, at, strict=True)):
            cx, cy = x0 + cx, row_y[r]
            w.shapes.append(box(cx, cy, cw, ch, CHIP_H / 2, D["surface"], D["line"]))
            mid = cy + CHIP_H / 2
            if kind in ("claude", "person", "other"):
                ink = {"claude": D["claude"], "person": D["success"], "other": D["secondary"]}[kind]
                w.shapes.append(dot(cx + CHIP_PAD + 5, mid, 5, ink))
            elif kind == "repo":
                w.shapes.append(box(cx + CHIP_PAD, mid - 5, 10, 10, 2, "none", D["secondary"], 1.5))
            ty = cy + (CHIP_H - text_h) / 2 if text_h + 8 <= CHIP_H else cy + 4
            w.texts.append(para(cx + CHIP_PAD + mark, ty, s, 11, D["text"], width - 2 * CHIP_PAD - mark)[0])
            if i == 0:
                w.at["caller"] = (cx, cy, cw, ch)
        y = yy - CHIP_GAP + SECTION_GAP

    # the relay's process tree with the box under its row, or the command in its box with colored words
    label, h, _ = para(x0, y, v["code_label"], 10, D["secondary"], width)
    w.texts.append(label)
    if v["tree"]:
        y = lay_out_tree(w, v, y + h + 6)
    else:
        y += h + 4
        code, code_h = code_text(x0 + CODE_PAD_X, y + CODE_PAD_Y, v["code"], width - 2 * CODE_PAD_X)
        box_h = code_h + 2 * CODE_PAD_Y
        w.shapes.append(box(x0, y, width, box_h, CODE_RADIUS, D["code_bg"], D["code_border"]))
        w.texts.append(code)
        y += box_h
        if v.get("code_note"):  # the root process and the rest of its line, small
            y += 4
            note, h = mono_para(x0, y, v["code_note"], 10, D["tertiary"], width)
            w.texts.append(note)
            y += h

    # the relay: what the signature is for, the destination and the key, the ssh command
    if v["facts"]:
        y += 12
        for kind, s in v["facts"]:
            if kind == "purpose":
                w.shapes.append(purpose_mark(x0, y))
                line, h, _ = para(x0 + MARK_W, y, s, 12, D["text"], width - MARK_W)
                y += h + 3
            elif kind == "mono":
                line, h = mono_para(x0 + MARK_W, y, s, 10, D["tertiary"], width - MARK_W)
                y += h + 2
            else:
                line, h, _ = para(x0 + MARK_W, y, s, 10, D["tertiary"], width - MARK_W)
                y += h + 2
            w.texts.append(line)

    # the seconds left to answer (the relay's window that asks), with the switch at the right of its row
    placed = False
    if v["countdown"]:
        y += SECTION_GAP
        w.at["bar"] = (x0, y, width, BAR_H)
        y += BAR_H + 7
        parts, halves = pill(lang, right, y)
        w.top.append(parts)
        w.at["pill"] = (halves[0][0], y, right - halves[0][0], PILL_H)
        w.at["ja"] = ((halves[1][0] + right) / 2, y + PILL_H / 2)
        placed = True
        template, status = v["countdown"]
        rest = f" · {status}" if status else ""
        cw = width - (right - halves[0][0]) - PILL_GAP
        # measured with the widest number (an answer wait of an hour), so that the row keeps its height
        _, h, _ = para(0, 0, template.replace("{0}", "0000") + rest, 10, "", cw)
        w.at["countdown"] = (x0, y + 5, cw, template, rest)
        y += max(h + 5, PILL_H)

    # the band: where 1Password's prompt usually comes, and the tray
    title_s, line_s, picture = v["band"]
    y += SECTION_GAP
    bx = x0 + BAND_PAD_X + SKETCH[0] + 12
    bw = right - BAND_PAD_X - bx
    _, th, _ = para(0, 0, title_s, 12, "", bw, bold=True)
    _, bl, _ = para(0, 0, line_s, 11, "", bw)
    inner = th + 3 + bl
    bh = max(inner, SKETCH[1]) + 2 * BAND_PAD_Y
    ty = y + (bh - inner) / 2
    w.shapes.append(box(x0, y, width, bh, BAND_RADIUS, D["band_bg"], D["band_border"]))
    w.shapes.append(sketch(picture, x0 + BAND_PAD_X, y + (bh - SKETCH[1]) / 2))
    w.texts.append(para(bx, ty, title_s, 12, D["band_title"], bw, bold=True)[0])
    w.texts.append(para(bx, ty + th + 3, line_s, 11, D["band_text"], bw)[0])
    y += bh

    # the footer with its shield, and the switch at the right of its row's bottom when the countdown has not taken it
    if v["footer"] or not placed:
        y += 12
        pill_w = 0.0
        if not placed:
            _, halves = pill(lang, 0, 0)
            pill_w = halves[1][1] - halves[0][0] + PILL_GAP
        h = 0.0
        if v["footer"]:
            _, h, _ = para(0, 0, v["footer"], 11, "", width - MARK_W - pill_w)
        row = max(h, PILL_H) if not placed else h
        ty = y + (PILL_H - h) / 2 if not placed and h < PILL_H else y
        if v["footer"]:
            w.shapes.append(shield(x0, ty + 1))
            w.texts.append(para(x0 + MARK_W, ty, v["footer"], 11, D["critical"], width - MARK_W - pill_w)[0])
        if not placed:
            top = y + row - PILL_H
            parts, halves = pill(lang, right, top)
            w.top.append(parts)
            w.at["pill"] = (halves[0][0], top, right - halves[0][0], PILL_H)
            w.at["ja"] = ((halves[1][0] + right) / 2, top + PILL_H / 2)
        y += row

    # the buttons (the relay's window that asks), styled by their place: the first (unless it is Deny) filled with the
    # accent color, its label bold; Deny outlined in the critical color, at the right end; any other outlined in the
    # accent color
    if v["buttons"]:
        y += BUTTON_TOP_GAP
        x = x0
        for n, (kind, label) in enumerate(v["buttons"]):
            filled = n == 0 and kind != "deny"
            tw = est_width(label, 12, filled)
            bw = max(tw + 2 * BUTTON_PAD_X, BUTTON_MIN_W)
            left = max(right - bw, x) if kind == "deny" else x
            if filled:
                w.top.append(box(left, y, bw, BUTTON_H, BUTTON_RADIUS, D["accent"]))
                ink = D["on_accent"]
            else:
                ink = D["critical"] if kind == "deny" else D["accent"]
                w.top.append(box(left, y, bw, BUTTON_H, BUTTON_RADIUS, D["surface"]))
                w.top.append(box(left + 0.5, y + 0.5, bw - 1, BUTTON_H - 1, BUTTON_RADIUS, "none", ink))
            w.top.append(wline(left + (bw - tw) / 2, y + (BUTTON_H - lh(12)) / 2, label, 12, ink, filled))
            w.at[kind] = (left, y, bw, BUTTON_H, ink)
            x = left + bw + BUTTON_GAP
        y += BUTTON_H
    w.h = y + PAD_BOTTOM
    return w


def frame_h(w: Laid) -> float:
    """The height of a window's frame around its client area: the border lines and the caption."""
    return 1 + WIN_CAPTION + w.h + 1


def window_svg(x: float, y: float, w: Laid, inner: str = "") -> str:
    """op-reason's window with its frame's top left at (x, y): one dark surface with round corners and Windows' thin
    border; its caption, in the window's color, shows the close button only. `inner`: drawn over the client area (in
    its DIPs)."""
    h = frame_h(w)
    gx, gy = x + WIN_W - 18, y + 1 + WIN_CAPTION / 2
    return (
        box(x, y, WIN_W, h, WIN_RADIUS, D["background"], more=' filter="url(#shadow)"')
        + f'<path d="M{num(gx - 4.5)},{num(gy - 4.5)}l9,9M{num(gx + 4.5)},{num(gy - 4.5)}l-9,9" fill="none" '
        f'stroke="{WIN_CLOSE}" stroke-width="1.1"/>'
        + f'<g transform="translate({num(x + 1)},{num(y + 1 + WIN_CAPTION)})">{inner}</g>'
        + box(x + 0.5, y + 0.5, WIN_W - 1, h - 1, WIN_RADIUS - 0.5, "none", WIN_BORDER)
    )


def light(r: tuple, attr: str, pad: float = 3) -> str:
    """A highlight over a part of the window that the scene points at (the caller's chip, the reason, the switch),
    `pad` above and below it."""
    x, y, w, h = r[:4]
    return (
        f'<rect x="{num(x - 4)}" y="{num(y - pad)}" width="{num(w + 8)}" height="{num(h + 2 * pad)}" rx="7" '
        f'fill="{BAND}" fill-opacity=".18" stroke="{BAND}" stroke-width="1.5"{attr}/>'
    )


def split_time(time: str) -> tuple:
    """look.rs: split_time — `YYYY-MM-DD HH:MM:SS` as the time and the date."""
    day, _, clock = time.partition(" ")
    return clock, day


def op_view(lang: str, call: dict) -> dict:
    """ui.rs: view — the window of a call of op by Claude Code in the example repository: the reason first, the
    caller, the repository and the directory as chips, the command, the band and the footer."""
    t = WINDOW[lang]
    time, date = split_time(call["time"])
    return {
        "icon": "lock",
        "title": t["title"],
        "subtitle": t["subtitle"],
        "time": time,
        "date": date,
        "reason_label": t["reason"],
        "reason": call["reason"],
        "chips": [("claude", call["caller"]), ("repo", f"{REPO} · {BRANCH}"), ("folder", PROMPT_PATH)],
        "code_label": t["command"],
        "code": call["command"],
        "code_note": None,
        "tree": None,
        "facts": [],
        "countdown": None,
        "band": (t["band_title"], t["band_line"], "op"),
        "footer": t["footer"],
        "buttons": [],
    }


def op_window(c: Canvas, x: float, y: float, lang: str, call: dict, lights: tuple = ()) -> tuple:
    """op-reason's window of `call` in `lang` with its frame's top left at (x, y). `lights`: (part, from, to) to light
    up the caller's chip ("caller"), the reason ("reason") or the switch ("pill").

    Returns the parts, the height and the middle of 日本語 in the switch (where a click switches an English window to
    Japanese).
    """
    w = lay_out(op_view(lang, call), lang)
    # the reason's line box is tall enough: the highlight keeps clear of its label above
    over = "".join(light(w.at[part], c.m.band(start, end), 1 if part == "reason" else 3) for part, start, end in lights)
    ja = w.at["ja"]
    return window_svg(x, y, w, w.svg() + over), frame_h(w), (x + 1 + ja[0], y + 1 + WIN_CAPTION + ja[1])


def mini_window(x: float, y: float, w: float, lang: str, caller: str, reason: str) -> str:
    """op-reason's window, small: its frame, the tile and the title, the reason and the caller's chip."""
    t = WINDOW[lang]
    h, cap, k = 86, 17, 18 / TILE  # the window, its caption, the tile at this scale
    gx, gy = x + w - 11, y + cap / 2 + 0.5
    tx, ty = x + 9, y + cap + 4
    chip_w, cy = 2 * 7 + 11 + est_width(caller, 9), y + 63
    return (
        box(x, y, w, h, 6, D["background"], more=' filter="url(#shadow)"')
        + f'<path d="M{num(gx - 3.5)},{num(gy - 3.5)}l7,7M{num(gx + 3.5)},{num(gy - 3.5)}l-7,7" fill="none" '
        f'stroke="{WIN_CLOSE}"/>'
        + box(tx, ty, 18, 18, TILE_RADIUS * k, D["accent"])
        + box(tx + 9.5 * k, ty + 7 * k, 9 * k, 10 * k, 4.5 * k, "none", D["on_accent"], 2 * k)
        + box(tx + 8 * k, ty + 13 * k, 12 * k, 9 * k, 2 * k, D["on_accent"])
        + wline(tx + 25, ty + (18 - lh(10.5)) / 2, t["title"], 10.5, D["text"], True)
        + wline(x + 9, y + 41, reason, 12.5, D["text"], True)
        + box(x + 9, cy, chip_w, 17, 8.5, D["surface"], D["line"])
        + dot(x + 9 + 7 + 3.5, cy + 8.5, 3.5, D["claude"])
        + wline(x + 9 + 7 + 11, cy + (17 - lh(9)) / 2, caller, 9, D["text"])
        + box(x + 0.5, y + 0.5, w - 1, h - 1, 5.5, "none", WIN_BORDER)
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

PROMPT_W, PROMPT_H = 288, 96  # the approval prompt of the scenes of a call


def op_call(lang: str, key: str, clock: str) -> dict:
    """An example call of op by Claude Code at `clock`, its reason REASONS[lang][key] ("proxmox" reads PROXMOX, "db"
    reads DB)."""
    item = PROXMOX if key == "proxmox" else DB
    return {
        "time": f"{DAY} {clock}",
        "caller": "Claude Code (pid 4242)",
        "reason": REASONS[lang][key],
        "command": f"op read {item}",
    }


def call_screen_h() -> float:
    """The screen of the scenes of a call (one call, the language), down to the bottom of the taskbar: tall enough for
    the approval prompt in its middle to clear the bottom of op-reason's window, the tallest the scenes show."""
    tallest = max(
        frame_h(lay_out(op_view(lang, op_call(said, key, "10:42:07")), lang))
        for lang in LANGS  # the window's language
        for said in LANGS  # the reason's
        for key in ("proxmox", "db")
    )
    return 2 * math.ceil(WIN_TOP + tallest + 14 + PROMPT_H / 2)


def scene_call(lang: str) -> str:
    """One call: the window, the prompt, the value, the sign-out and the log."""
    tx = TEXTS[lang]
    reason = REASONS[lang]["proxmox"]
    d = 13.5
    screen_h = call_screen_h()
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

    # the 1Password approval prompt (a plain box) in the middle of the screen, op-reason's window at its top right: who
    # calls (the caller's chip) and why (the reason) light up in turn
    pw, ph = PROMPT_W, PROMPT_H
    prompt, approve = prompt_box(m, (W - pw) / 2, (screen_h - ph) / 2, pw, ph, lang, (t_click,))
    c.add(g(m.at(t_prompt, t_click + 0.15, 0.2), prompt))
    call = op_call(lang, "proxmox", "10:42:07")
    lights = (("caller", 2.3, 3.1), ("reason", 3.2, 4.5))
    win, _, _ = op_window(c, W - WIN_RIGHT - WIN_W, WIN_TOP, lang, call, lights)
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
        prompt, approve = prompt_box(
            m, px + (pw - bw) / 2, sy + (sh - bh) / 2, bw, bh, lang, tuple(presses), small=True
        )
        c.add(g(m.shown(spans, 0.2), prompt))
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
    screen_h = call_screen_h()  # as in the one-call scene: the prompt in the middle clears the window
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
    pw, ph = PROMPT_W, PROMPT_H
    px, py = (W - pw) / 2, (screen_h - ph) / 2
    prompt, approve = prompt_box(m, px, py, pw, ph, lang, (t_click,), focus=True)
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
        g(m.shown([(t_prompt, t_click + 0.15), (t_window2 + 0.15, None)], 0.2), prompt, glow),
        g(m.at(t_switch + 0.15, t_click, 0.2), badge),
    )

    # op-reason's window at the top right: English, then Japanese at once after the click (it keeps its top right
    # corner and takes its new height); the next call's window opens in Japanese, its switch lit up
    wx = W - WIN_RIGHT - WIN_W
    first, second = op_call(lang, "proxmox", "10:42:07"), op_call(lang, "db", "10:43:18")
    win_en, _, link = op_window(c, wx, WIN_TOP, "en", first)
    win_ja, _, _ = op_window(c, wx, WIN_TOP, "ja", first)
    win_next, _, _ = op_window(c, wx, WIN_TOP, "ja", second, (("pill", t_window2 + 0.5, 11.6),))
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

    rest = (800, py + ph / 2)  # the mouse before and after: on the desktop, under the window and right of the prompt
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
    """A card of the simulation where pass_window.rs draws it, moving and fading as simulated, in the dark look: shadow,
    glow, the surface with the line's border, the check in the success color, its texts over time and the line of its
    four seconds (in steps of half a second). The card is at 90% (an inner group) times its fade (the outer); the ring
    of its glow is outside the 90%, at full strength, and fades with the card."""
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
        f'fill-opacity="{TOAST_SHADOW}"/>',
        box(x, 0, CARD_W, CARD_H, 6, D["surface"]),
        box(x + 0.5, 0.5, CARD_W - 1, CARD_H - 1, 6, "none", D["line"]),
        dot(x + 18, 17.5, 8, D["success"]),
        f'<path d="M{num(x + 14)},17.8L{num(x + 16.8)},20.6L{num(x + 22.2)},14.5" fill="none" '
        f'stroke="{D["on_success"]}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>',
        f'<rect x="{num(lx)}" y="{num(CARD_H - 5)}" width="{num(TOAST_TEXT_W)}" height="2" fill="{D["success"]}"'
        f'{m.track(bar, cls="tl")}/>',
    ]
    for k, (t, texts, n) in enumerate(card.variants):
        until = card.variants[k + 1][0] if k + 1 < len(card.variants) else card.gone
        # line 1: "Passed" in the success color, then " · <caller>" in the text color where it ends
        word_w = est_width(texts["word"], TOAST_BODY, True)
        head_w = word_w + est_width(f" · {texts['caller']}", TOAST_BODY, True)
        line1 = "".join(
            f'<text x="{num(at)}" y="22" font-size="{num(TOAST_BODY)}" fill="{ink}" class="b" '
            f'textLength="{num(est_width(part, TOAST_BODY, True))}" lengthAdjust="spacing" xml:space="preserve">'
            f"{esc(part)}</text>"
            for at, part, ink in (
                (lx, texts["word"], D["success"]),
                (lx + word_w, f" · {texts['caller']}", D["text"]),
            )
        )
        chip = ""
        if n >= 2:
            label = f"×{n}"
            cx, cw = lx + head_w + 6, est_width(label, TOAST_SMALL, True)
            chip = box(cx, 10, cw + 10, 15, 4, D["success_tint"]) + (
                f'<text x="{num(cx + 5)}" y="22" font-size="{num(TOAST_SMALL)}" fill="{D["success"]}" class="b" '
                f'textLength="{num(cw)}" lengthAdjust="spacing">{esc(label)}</text>'
            )
        # line 2 in the monospace font: the destination whole (at most 60% of the line), the command cut before it
        adv = TOAST_SMALL * ADVANCE
        dest = col_fit(f" → {texts['dest']}", int(TOAST_TEXT_W * TOAST_DEST_SHARE / adv))
        command = col_fit(texts["command"], int((TOAST_TEXT_W - cols(dest) * adv) / adv))
        dest_x = lx + (TOAST_TEXT_W - cols(dest) * adv if command != texts["command"] else cols(command) * adv)
        why = fit(texts["why"], TOAST_SMALL, TOAST_TEXT_W)
        lines = (
            line1
            + chip
            + mono(lx, 39, command, TOAST_SMALL, D["text"])
            + mono(dest_x, 39, dest, TOAST_SMALL, D["text"])
            + f'<text x="{num(lx)}" y="55" font-size="{num(TOAST_SMALL)}" fill="{D["secondary"]}" '
            f'textLength="{num(est_width(why, TOAST_SMALL))}" lengthAdjust="spacing" xml:space="preserve">'
            f"{esc(why)}</text>"
        )
        parts.append(g(m.at(t / 1000, None if until is None else until / 1000), lines))
    ring = (
        f'<rect x="{num(x - 2.5)}" y="-2.5" width="{num(CARD_W + 5)}" height="{num(CARD_H + 5)}" rx="8.5" fill="none" '
        f'stroke="{D["success"]}" stroke-width="2.5" stroke-opacity=".9"{glow_attr}/>'
    )
    inner = g(m.keyframes(ostops), g(f' style="opacity:{num(OPACITY)}"', *parts), ring)
    return f'<g transform="translate(0,{num(rest_y)})">{g(m.keyframes(ystops), inner)}</g>'


def col_fit(s: str, n: int) -> str:
    """`s` cut with an ellipsis to `n` columns of the monospace font, as DirectWrite trims a line of the toast."""
    if cols(s) <= n:
        return s
    while s and cols(s) + 1 > n:
        s = s[:-1]
    return s + "…"


def more_tag(c: Canvas, steps: list) -> str:
    """The tag +N where pass_window.rs draws it (under the third place, at 90%), one per value of N while it holds: the
    surface with the line's border, +N in the secondary color."""
    m = c.m
    x, top = EDGE, toast_more_y()
    out = []
    for k, (t, n) in enumerate(steps):
        if n == 0:
            continue
        until = steps[k + 1][0] if k + 1 < len(steps) else None
        label = f"+{n}"
        tw = est_width(label, TOAST_SMALL, True)
        w = tw + 2 * TOAST_MORE_PAD
        r = TOAST_MORE_RADIUS
        out.append(
            g(
                m.at(t / 1000, None if until is None else until / 1000),
                f'<rect x="{num(x - 1)}" y="{num(top + 1)}" width="{num(w + 2)}" height="{num(MORE_H + 1)}" '
                f'rx="{num(r + 1)}" fill="#000" fill-opacity="{TOAST_SHADOW}"/>',
                box(x, top, w, MORE_H, r, D["surface"]),
                box(x + 0.5, top + 0.5, w - 1, MORE_H - 1, r, "none", D["line"]),
                f'<text x="{num(x + TOAST_MORE_PAD)}" y="{num(top + 12)}" font-size="{num(TOAST_SMALL)}" '
                f'fill="{D["secondary"]}" class="b" textLength="{num(tw)}" lengthAdjust="spacing">{esc(label)}</text>',
            )
        )
    return g(f' style="opacity:{num(OPACITY)}"', *out) if out else ""


KEY_FINGERPRINT = "SHA256:Jq7c0R…"  # the key of the example's signature (made up)


def relay_view(lang: str, call: dict) -> dict:
    """relay_ui.rs: view — the window that asks (per-call) of a call of Claude Code's Bash tool on Windows, which has a
    root and its processes: the reason first, the caller's chip, the process tree (tree_view: Claude Code with its image
    and PID, the root with its mark and, under its row, the root command in the box and the rest of its line, the
    client with its ssh command small under its row, and the frame of what Allow this call covers, from the root
    down), what the signature is for, the destination with its host key and the key, the countdown and how long an
    answer lasts (until the root command's program ends), the band, then Allow this call and Deny."""
    t = RELAY[lang]
    time, date = split_time(call["time"])
    name, pid, frame, command = call["root"]
    client, client_pid = call["client"]
    rows = [  # the caller (its dot, its image and PID), the root (its mark) and the client, each in bold
        {"depth": 0, "dot": "claude", "name": t["claude"], "detail": f"claude.exe · pid {call['pid']}"},
        {"depth": 1, "name": name, "detail": f"pid {pid}", "mark": t["root_mark"]},
        {"depth": 2, "name": client, "detail": f"pid {client_pid}"},
    ]
    rows = [{**row, "tone": "strong"} for row in rows]
    detail = " · ".join(
        [
            f"{t['destination']} {call['dest']} · {t['host_key'].replace('{0}', HOST_KEY)}",
            f"{t['key']} {KEY_FINGERPRINT}",
        ]
    )
    return {
        "icon": "key",
        "title": t["title"],
        "subtitle": t["subtitle"],
        "time": time,
        "date": date,
        "reason_label": t["reason"],
        "reason": call["reason"],
        "chips": [("claude", t["claude_pid"].replace("{0}", call["pid"]))],
        "code_label": t["tree_label"],
        "code": command,
        "code_note": frame,
        "tree": {
            "rows": rows,
            "boxed": 1,
            "small": (2, call["command"]),
            "frames": [{"first": 1, "last": 2, "label": t["frame_call"], "solid": True}],
        },
        "facts": [
            ("purpose", t["userauth"].replace("{0}", call["user"])),
            ("detail", detail),
        ],
        "countdown": (
            t["countdown"],
            t["covers_name"].replace("{0}", program_name(command)).replace("{1}", str(LIMIT_MINUTES)),
        ),
        "band": (t["band_title"], t["band_line"], "ask"),
        "footer": "",
        "buttons": [("allow", t["allow"]), ("deny", t["deny"])],
    }


def relay_window(c: Canvas, x: float, y: float, lang: str, call: dict, shown: tuple, press: float) -> tuple:
    """The relay's window that asks (per-call) with its frame's top left at (x, y), as window.rs draws the view of
    relay_ui.rs; its countdown runs while it is `shown` (from, to), redrawn every 250 ms, and Allow is pressed at
    `press` (a border 2 DIPs wide in its label's color, its fill and label as they are). Returns the parts, the height
    and the middle of Allow."""
    m = c.m
    w = lay_out(relay_view(lang, call), lang)
    opened, closed = shown
    bx, by, bw, bh = w.at["bar"]
    tick = COUNTDOWN_MS / 1000
    share = [(0.0, "transform:scaleX(1)")] + [
        (opened + k * tick, f"transform:scaleX({1 - k * tick / ANSWER_WAIT:.4f})")
        for k in range(1, math.ceil((closed - opened) / tick))
    ]
    countdown = [
        box(bx, by, bw, bh, BAR_H / 2, D["line"]),
        box(bx, by, bw, bh, BAR_H / 2, D["caution"], more=m.track(share, cls="tl")),
    ]
    lx, ly, lw, template, rest = w.at["countdown"]
    for k in range(math.ceil(closed - opened)):  # the seconds left, from the moment the window opens
        line = para(lx, ly, template.replace("{0}", str(ANSWER_WAIT - k)) + rest, 10, D["secondary"], lw)[0]
        until = opened + k + 1
        countdown.append(g(m.at(0.0 if k == 0 else opened + k, None if until >= closed else until), line))
    left, top, width, height, ink = w.at["allow"]
    press_attr = m.pulse(press, 0.25, SHOWN)
    pressed = box(left + 1, top + 1, width - 2, height - 2, BUTTON_RADIUS - 1, "none", ink, 2, press_attr)
    svg = window_svg(x, y, w, w.svg("".join(countdown)) + pressed)
    return svg, frame_h(w), (x + 1 + left + width / 2, y + 1 + WIN_CAPTION + top + height / 2)


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


NOTE_W, NOTE_H = 320, 96  # a Windows notification


def relay_call(lang: str) -> dict:
    """The call that asks in the relay's scene: Claude Code on Windows (claude.exe, its PID in its chip and at the head
    of the tree) checks the service on web-02. Its Bash tool ran ssh in a bash (bash.exe), the call's root: the root's
    image, PID, its line with the command it runs as … (look.rs: root_command's frame) and that command; then the client
    that bash started (ssh.exe) and its PID: the chain ssh.exe → bash.exe → claude.exe of conn.rs's identify, with no
    process between. The client's command line is the toasts' too."""
    return {
        "time": f"{DAY} 10:42:07",
        "pid": "4242",
        "reason": RELAY_REASONS[lang]["service"],
        "root": ("bash.exe", "5150", 'bash -c -l "…"', 'ssh deploy@web-02 "systemctl status app"'),
        "client": ("ssh.exe", "5236"),
        "command": RELAY_CALLS["deploy"][1],
        "dest": RELAY_CALLS["deploy"][2],
        "user": "deploy",
    }


def relay_screen_h() -> float:
    """The screen of the relay's scene, down to the bottom of the taskbar: the window that asks at the top right, and
    the notification under it at the bottom right."""
    tallest = max(frame_h(lay_out(relay_view(lang, relay_call(lang)), lang)) for lang in LANGS)
    return math.ceil(WIN_TOP + tallest + 14 + NOTE_H + 12 + 28)


def scene_relay(lang: str) -> str:
    """The SSH relay: toasts at the top left (one per call, three at most, four seconds), and the window that asks at
    the top right with its notification at the bottom right, gone together."""
    tx = TEXTS[lang]
    r = RELAY[lang]
    d = 33.0
    screen_h = relay_screen_h()
    # without motion: the window asks, its notification is at the bottom right, and a toast is at the top left
    c = Canvas(screen_h + 44, d, rest=24.4)
    c.css.append(".tl{transform-box:fill-box;transform-origin:0 50%}")
    m = c.m
    c.add(desk(0, 0, W, screen_h), editor_window(80, 92, 440, screen_h - 200), taskbar(screen_h - 28, 28, "10:42"))
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
    call = relay_call(lang)
    reason, command = call["reason"], call["command"]
    win, _, allow = relay_window(c, W - WIN_RIGHT - WIN_W, WIN_TOP, lang, call, (t_open, t_close), t_press)
    slide = {"hidden": "opacity:0;transform:translateY(-8px)", "shown": "opacity:1;transform:translateY(0)"}
    c.add(g(m.at(t_open, t_close, 0.3, **slide), win))
    nw = NOTE_W
    note = notification(
        W - 12 - nw,
        screen_h - 28 - 12 - NOTE_H,
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

    rest = (500, screen_h - 60)  # the mouse before and after: on the desktop, above the taskbar
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
    """What the HTML page must hold: the times and counts of pass_toast.rs and of the window's countdown, the colors of
    the dark look, and the texts of the toast and of the window (the band's line by its runs: the page tints the marked
    words)."""
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
        f"ANSWER_WAIT = {ANSWER_WAIT}",
        f"LIMIT = {LIMIT_MINUTES}",
        f"COUNTDOWN_MS = {COUNTDOWN_MS}",
    ]
    # every role but those its window does not show (no path in its command, the picture's window is this one)
    unused = ("code_path", "pic_window")
    colors = [f"--op-{role.replace('_', '-')}: {rgb};" for role, rgb in DARK.items() if role not in unused]
    keys = ("title", "subtitle", "reason", "tree_label", "root_mark", "frame_call", "countdown", "covers_name")
    keys += ("band_title", "allow", "deny")
    keys += ("pass_word", "pass_call", "pass_terminal", "no_reason")
    texts = [RELAY[lang][k] for lang in LANGS for k in keys]
    for lang in LANGS:
        plain, tones = unmark(RELAY[lang]["band_line"])
        texts += ["".join(ch for ch, _ in run) for _, run in itertools.groupby(zip(plain, tones), key=lambda q: q[1])]
    return times + colors + texts


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


def squeeze(s: str) -> str:
    """`s` with each run of white space as one space: a needle may span the lines rustfmt breaks a statement into."""
    return " ".join(s.split())


def check_sources() -> list:
    problems = []
    for rel, needles in sources().items():
        body = squeeze((ROOT / rel).read_text(encoding="utf-8"))
        problems += [f"{rel} no longer has {n!r}" for n in needles if squeeze(n) not in body]
    # the colors: in the DARK palette, not merely somewhere in the file
    theme = (ROOT / THEME).read_text(encoding="utf-8")
    start = theme.find("pub const DARK: Palette = Palette {")
    dark = theme[start : theme.find("};", start)] if start >= 0 else ""
    problems += [f"{THEME}: DARK no longer has {n.strip()!r}" for n in dark_needles() if n not in dark]
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
