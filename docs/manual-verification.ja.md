# 本物の 1Password で op-reason を確かめる

English: [manual-verification.md](manual-verification.md)

CI は偽の `op-reason.exe`・`powershell.exe`・`op.exe` でしか op-reason を流せません (1Password のデスクトップアプリは CI で
動かず、CI の Windows の runner では WSL2 も動きません)。ここの確認には、人と、1Password のアプリと 1Password CLI の入った
Windows と、WSL が要ります。WSL の確認 (1〜14 と R) で 30 分ほどかかり、承認の画面が 15 回ほど出ます。
参照の一覧の確認 (F) は 10 分ほど足します。

## 確かめる間の決まり

- 出力にシークレットの無いコマンドだけを使います: `vault list`・`whoami`・`item template get`・`item create --dry-run`。
  手順に無い限り、出力は `/dev/null` に捨てます。`op read`・`item get`・`--reveal` は打ちません。
- 画面には手順どおりに応えます。手順が見込んでいない画面が出たら拒否し、書き留めます。
- 終わったら 1Password を 1 回ロックします。

## 用意

Windows の zip (Release の `op-reason-windows.zip`、か pull request の Windows exe の workflow の `op-reason-windows`
artifact) を Windows のフォルダーに展開し、README のとおりに SHA-256 を確かめます。WSL では:

```sh
cargo build --release
export PATH=$PWD/target/release:$PATH
ln -sf op-reason target/release/op && ln -sf op-reason target/release/op-reason-log
command -v op            # …/target/release/op
op.exe --version         # 本物の 1Password CLI が答える
export OPREASON_EXE=/mnt/c/Users/<あなた>/<展開したフォルダー>/op-reason.exe   # PATH にあれば要らない
export OPREASON_LANG=ja  # 英語なら en
```

`cargo build` が ``linker `cc` not found`` で止まる (C のコンパイラーが無い) マシンでは、代わりに repo の開発用の
shell の中で build します: `nix develop "path:$PWD/nix" --command cargo build --release`。

1〜14 は既定 (`OPREASON_RUNNER` なし) のまま、`op-reason.exe` を通して確かめます。窓は Direct2D の窓です
(launcher の WPF の窓と見た目が少し違います)。

## 確かめること

| # | すること | 見込み |
| --- | --- | --- |
| 1 | `op --reason "確認 1: vault の一覧" vault list >/dev/null`、許可 | 画面の右上に窓 (時刻・呼び手 (人)・場所・理由・コマンド) と toast が出て、画面は 1 回、終了コード 0。`op-reason-log` に `許可` |
| 2 | すぐに 1 をもう一度、許可 | 画面がまた出る (1 回目の承認を使い回さない) |
| 3 | `op.exe whoami; echo $?` (op-reason を通さず直に) | sign in していない、終了コードは 0 以外 (WSL に承認が残っていない) |
| 4 | `OPREASON_LANG=en op --reason "check 4: English window" vault list >/dev/null`、許可 | 窓と toast が英語で出る |
| 5 | `op --reason "確認 5: 拒否" vault list`、**拒否** | stderr に `[ERROR] … authorization prompt dismissed`、終了コードは 0 以外。`op-reason-log` に `拒否` |
| 6 | `op --reason "確認 6: 時間切れ" vault list`、1Password があきらめるまで応えない | `op-reason-log` に `時間切れ` |
| 7 | 端末 A: `op --reason "確認 7A" vault list >/dev/null`、まだ応えない。端末 B: `op --reason "確認 7B" vault list >/dev/null` | B が「ほかの op の承認待ちが終わるのを待っています」と出す。A を許可すると B の窓と画面が出るので、B も許可 |
| 8 | 7 と同じで、B に `OPREASON_LOCK_WAIT=10` を付け、A に 15 秒応えず、そのあと A を拒否 | B は A の理由を出して 75 で終わる。`op-reason-log` に `断った (承認待ちが終わらない)` |
| 9 | `op --reason "確認 9: 雛形を読む" item template get Login \| op --reason "確認 9: 雛形から項目を試しに作る" item create --dry-run --vault Private - >/dev/null`、画面は 2 回とも許可 | 窓が 2 つ出て、止まらずに終わり、終了コード 0。`op-reason-log` に `許可` が 2 つ。受け手が先に lock を取ったときは、送り手の窓に lock を待たずに打つ注意が出て、その行は `(lock を待たず)` |
| 10 | `op.exe vault list >/dev/null` (直に)、許可。続けて `op --reason "確認 10: 見張り" whoami` | 打つ前に共有の承認を消す: 通知「共有の 1Password CLI の承認を消しました」。`op-reason-log` に `共有の承認を消した` (そのあとの `whoami` は sign in していないと答える) |
| 11 | `timeout 5 op --reason "確認 11: 止める" vault list`、応えない | 窓と画面がどうなるかを書き留める (R3 と同じ見方)。画面が残れば拒否する。`op-reason-log` に `中断` |
| 12 | `op --version`・`op --help`・`op item get --help` | 理由なしで通り、窓も画面も出ない |
| 13 | `op item get x` (理由なし) | 終了コード 2、AI エージェント向けの案内つきの使い方。`op-reason-log` に `断った (理由なし)` |
| 14 (任意) | service account があれば、token を表示せずに `OP_SERVICE_ACCOUNT_TOKEN` に入れ、`op --reason "確認 14: service account" vault list >/dev/null` | 窓も画面も出ず、終了コード 0。`op-reason-log` に `成功 (SA)` |

## WSL から op-reason.exe を通すときに確かめること

CI では WSL から Windows の program を起こせないので、ここは手でしか確かめられません。約 15 分かかり、画面は 6 回ほど
出ます。見込みに「書き留める」とある所は、まだ誰も見ていないことです。見たとおりに書いてください。

| # | すること | 見込み |
| --- | --- | --- |
| R1 | `time op --version` を 3 回、続けて `OPREASON_RUNNER=launcher` を付けて 3 回 (理由も画面も要らない呼び出し) | それぞれの `real` を書き留める。exe の方が launcher よりずっと短い見込み (Windows から直に起こすと exe は数十ミリ秒、launcher は 0.5 秒ほど) |
| R2 | `op --reason "確認 R2: 窓までの時間" vault list >/dev/null`、許可。続けて `OPREASON_RUNNER=launcher` を付けて同じことをする | Enter から窓が出るまでの感じ (すぐ・0.5 秒ほど・1 秒以上) を両方書き留める |
| R3 | `op --reason "確認 R3: TERM" vault list & pid=$!`、画面が出たら応えずに `kill -TERM $pid` | 書き留める: 窓は消えたか、画面は残ったか、Windows の `tasklist.exe /FI "IMAGENAME eq op-reason.exe"` と `tasklist.exe /FI "IMAGENAME eq op.exe"` に残ったものはあるか。`op-reason-log` に `中断` (143)。画面が残れば拒否する。`OPREASON_RUNNER=launcher` でも同じことをし、違いを書き留める |
| R4 | `op --reason "確認 R4: Ctrl+C" vault list` の画面が出ている間に、WSL の端末で Ctrl+C | R3 と同じ見方で書き留める。`op-reason-log` に `中断` (130)。続けて `op.exe whoami` が sign in していないと出すか |
| R5 | Smart App Control が `op-reason.exe` を止めるマシンで `op --reason "確認 R5: 回避" vault list >/dev/null`、許可。止めないマシンでは、`C:\Windows\System32\whoami.exe` を `op-reason.exe` という名前で空のフォルダーに写し、そこに zip の `launcher.ps1` を置き、`OPREASON_EXE` をその `op-reason.exe` に向けて同じことをする | WSL に出た文をそのまま書き留める (止められたときの interop の文があれば)。続けて「launcher.ps1 で打ち直します」の 1 行が出て、launcher の窓が出る。許可すると終了コード 0。`op-reason-log` に `許可 (回避経路)`、`op-reason-log --json` の最後の呼び出しの `end` に `fallback` |
| R6 | R5 の形のまま `printf 'plain text\n' \| op --reason "確認 R6: 回避でも stdin" inject`、許可 | `plain text` と出る (回避しても stdin が欠けずに届く) |
| R7 | R5 の形のまま `OPREASON_RUNNER=launcher op --reason "確認 R7: launcher だけ" vault list >/dev/null`、許可 | 「打ち直します」の行が出ずに launcher の窓が出る (Smart App Control の通知も出ない) |

## 窓の表示の言語を確かめること

既定は英語で、`op-reason.exe` の窓の右下の切り替え `English | 日本語` で選びます
([2026-11-15-language-toggle-design.md](superpowers/specs/2026-11-15-language-toggle-design.md))。CI は Windows の runner の
マウスで切り替えを押しますが、その横に 1Password の画面は出せません。約 10 分かかり、画面は 7 回ほど出ます。始める前に、
WSL で `unset OPREASON_LANG; rm -f ~/.local/state/op-reason/lang.json`、PowerShell で
`Remove-Item "$env:LOCALAPPDATA\op-reason\settings.json" -ErrorAction SilentlyContinue` を打ちます。

| # | すること | 見込み |
| --- | --- | --- |
| L1 | `LANG=ja_JP.UTF-8 op --reason "check L1: English by default" vault list >/dev/null`、許可 | 窓と通知が英語 (`LANG` では日本語にならない)。窓の右下に `English \| 日本語`。English は黒の太字、日本語は青の下線つき。マウスの矢印は日本語の上でだけ手の形になる |
| L2 | `op --reason "確認 L2: 切り替え" vault list >/dev/null`。画面が出ている間に op-reason の窓の日本語を押し、応える前に画面の様子を見る | 窓がその場で日本語になり (題・ラベル・足元の行。今度は日本語が太字で English が押せる側)、右上の位置は変わらない。フォーカスを奪わない: 画面が前面の窓のまま (画面の題の帯は前面の色のまま、op-reason の窓の題の帯は灰色のまま) で、キーボードも画面に届く (文字を入れる欄があれば、そこでカーソルが点滅し続ける。パスワードは打たない)。許可する。`Get-Content "$env:LOCALAPPDATA\op-reason\settings.json"` が `{"v":1,"lang":"ja"}` |
| L3 | `op --reason "確認 L3: 覚えている" vault list >/dev/null`、許可。続けて `op item get x` (理由なし) と `op-reason-log` | 窓と通知が始めから日本語。断るときの文と `op-reason-log` も日本語。`cat ~/.local/state/op-reason/lang.json` が `{"v":1,"lang":"ja"}` |
| L4 | `OPREASON_LANG=en op --reason "確認 L4: 選んだ方が OPREASON_LANG より先" vault list >/dev/null`、許可 | 日本語のまま |
| L5 | `op --reason "確認 L5: 英語に戻す" vault list >/dev/null` の窓で English を押し、許可。続けて PowerShell で `op-reason.exe --reason "check L5 from Windows" vault list > $null`、許可 | Windows から呼んだ窓が英語で、切り替えもある (WSL と Windows で選んだ言語は 1 つ)。`op-reason.exe --log` も英語 |
| L6 | `OPREASON_RUNNER=launcher OPREASON_LANG=ja op --reason "確認 L6: launcher も選んだ言語を読む" vault list >/dev/null`、許可 | launcher の窓 (WPF の方) が、選んだ言語の英語で出て、切り替えは無い |
| L7 | この節の始めのとおりに 2 つのファイルを消す。`OPREASON_LANG=ja op --reason "確認 L7: 戻す" vault list >/dev/null`、許可。`unset OPREASON_LANG` してもう一度、許可 | 1 回目は日本語 (選んでいない間は `OPREASON_LANG` が効く)、2 回目は英語 |

## 窓の参照の一覧を確かめること

窓には呼び出しが読む `op://` の参照が並び、ID の参照を押すと 1Password のアプリでその項目が開きます
([2026-11-17-refs-in-window-design.md](superpowers/specs/2026-11-17-refs-in-window-design.md))。CI は偽の `op.exe` と偽の
`onepassword:` の受け口で一覧・アカウントの控え・クリックを確かめますが、本物のアプリでは確かめられません。10 分ほどかかり、
承認の画面が 7 回ほど出ます。F2 のほかは承認の画面を**拒否**します (窓は画面より先に出るので、何も読みません)。
`inject` の行は op の出力を `/dev/null` (PowerShell では `$null`) に捨てます。誤って許可しても、値は端末に出ません。

最初に: 1Password で見られて困らない項目を選び、「…」メニューの「プライベートリンクをコピー」を押して、`v=` の後ろの値 (vault
の ID) と `i=` の後ろの値 (項目の ID) を控えます (リンクはほかに貼りません)。WSL の空のフォルダで
`V=<v の値>; I=<i の値>` と
`printf 'A={{ op://%s/%s/password }}\nB={{ op://Private/no-such-item/password }}\n' "$V" "$I" > tpl.env` を打ちます。
PowerShell で `Remove-Item "$env:LOCALAPPDATA\op-reason\account.json" -ErrorAction SilentlyContinue` を打ちます。

| # | すること | 期待すること |
| --- | --- | --- |
| F1 | `op --reason "check F1: references, first call" inject -i tpl.env >/dev/null`、拒否 | コマンドの行の下に「読む参照 (2 件・tpl.env から):」、字下げした 2 つの参照が黒で (リンクなし)、「項目へのリンクは、承認が通った呼び出しを 1 回終えると付きます (…)」の行 |
| F2 | `op --reason "check F2: learn the account" vault list >/dev/null`、許可 | `Get-Content "$env:LOCALAPPDATA\op-reason\account.json"` が `{"v":1,"accounts":[{"id":"…","host":"…"}]}` を出す。`id` はプライベートリンクの `a=`、`host` はサインインのアドレスで、ほかには何も無い (メールアドレスも無い) |
| F3 | `op --reason "check F3: a link" inject -i tpl.env >/dev/null`。1 つ目の参照は青の下線付きで、上に置くと手のポインタになる。2 つ目は黒のまま。リンクが付かない旨の行は無い。承認の画面が出ている間に 1Password で別の項目を選んでから、リンクを押す。承認の画面を見てから拒否 | 1Password のアプリがリンクの項目を開いて前に出る。承認の画面はアプリの手前に残り、まだ答えられる。op-reason の窓はフォーカスを取らない (タイトルバーが灰色のまま) |
| F4 | 1Password をロックし、F3 のコマンドをもう一度打ち、承認の画面がロックの解除を求めている間にリンクを押す | 何も前に出ない: アプリはロックのまま、後ろにある。承認の画面は残り、まだ答えられる。拒否 |
| F5 | PowerShell で、`tpl.env` を写したフォルダで `Get-Content tpl.env \| op-reason.exe --reason "check F5: a template on stdin" inject > $null`、拒否 | 「読む参照 (2 件・標準入力 から)」とリンク (exe が stdin のテンプレートを読んだ) |
| F6 | `OPREASON_RUNNER=launcher op --reason "check F6: the launcher" inject -i tpl.env >/dev/null`、リンクを押し、拒否 | launcher の (WPF の) 窓にも同じ一覧とリンクが出る。押すとアプリが項目を開いて前に出て、承認の画面はその手前に残る。launcher の窓は一度も前面の窓にならない |
| F7 | `op-reason-log`、続けて `op-reason-log --json \| tail -n 1` | F1・F3・F4・F6 の行の末尾に「(参照 2 件)」。最後の JSON の `refs.list` に 2 つの参照がある |

## Windows で確かめること: op-reason.exe

R の確認で展開したフォルダーの `op-reason.exe` を使います。`op-reason.exe` と名前を全部書いて呼びます。約 15 分かかり、
画面は 10 回ほど出ます。

| # | すること | 見込み |
| --- | --- | --- |
| E1 | PowerShell で `op-reason.exe --version` | 版が出る。Windows が起こすのを断った (Smart App Control) か警告を出した (SmartScreen) ときは、出た文をそのまま書き留める。ほかの E は確かめられず、このマシンでは PowerShell の入口 (W の確認) を使う |
| E2 | `op-reason.exe --reason "確認 E2: vault の一覧" vault list > $null`、許可 | 右上に窓と toast、画面は 1 回、終了コード 0。`op-reason.exe --log` に `許可` |
| E3 | E2 をもう一度 | 画面がまた出る |
| E4 | cmd.exe で `op-reason.exe --reason "確認 E4: cmd から" whoami`、許可 | cmd.exe からも同じく動く |
| E5 | 端末 A (WSL): `op --reason "確認 E5 WSL" vault list >/dev/null`、まだ応えない。端末 B (PowerShell): `op-reason.exe --reason "確認 E5 exe" vault list > $null` | B はほかの op の呼び出しを待つと出す。A に応えると B の窓と画面が出る |
| E6 | E5 と同じで、B に `$env:OPREASON_LOCK_WAIT = 10` を付け、A に 15 秒応えず、そのあと A を拒否 | B は A の理由を出して 75 で終わる。`op-reason.exe --log` で B は `断った (承認待ちが終わらない)` |
| E7 | `op-reason.exe --reason "確認 E7: 拒否" vault list`、**拒否** | 終了コードは 0 以外で、op の誤りが stderr に出る。`op-reason.exe --log` に `拒否` |
| E8 | `op-reason.exe --reason "確認 E8: 止める" vault list` の画面が出ている間に Ctrl+C | 窓が消える。画面が残れば拒否する。`op-reason.exe --log --json` に `interrupted` と終了コード 130 があり、`op.exe whoami` は sign in していないと出す |
| E9 | `op-reason.exe --reason "確認 E9: 止める" vault list` の画面が出ている間に、タスク マネージャーで `op-reason.exe` を終了する | 窓が消え、`op.exe` も終わって画面が消える (Job Object)。消えなければ書き留めて、画面を拒否する |

## Windows で確かめること: PowerShell の入口 (回避経路)

zip の `powershell-entry` フォルダーを、`PATH` の `op-reason.exe` のフォルダーより前に足して使います (README の
「Smart App Control に op-reason.exe を止められたら」)。

| # | すること | 見込み |
| --- | --- | --- |
| W1 | PowerShell で `op-reason --reason "確認 W1: vault の一覧" vault list > $null`、許可 | 窓と toast、画面は 1 回、終了コード 0。`%LOCALAPPDATA%\op-reason\log.jsonl` に `start` と `end` (`approved`) |
| W2 | W1 をもう一度 | 画面がまた出る |
| W3 | cmd.exe で `op-reason.cmd --reason "確認 W3: cmd から" whoami`、許可 | shim を通しても同じく動く |
| W4 | 端末 A (WSL): `op --reason "確認 W4 WSL" vault list >/dev/null`、まだ応えない。端末 B (PowerShell): `op-reason --reason "確認 W4 Windows" vault list > $null` | B はほかの op-reason の呼び出しを待つと出す。A に応えると B の窓と画面が出る |
| W5 | W4 と同じで、B に `$env:OPREASON_LOCK_WAIT = 10` を付け、A に 15 秒応えず、そのあと A を拒否 | B は A の理由を出して 75 で終わる。B の `end` は `lock-timeout` |
| W6 | `op-reason --reason "確認 W6: 拒否" vault list`、**拒否** | 終了コードは 0 以外で、op の誤りが stderr に出る。`end` は `denied` |
| W7 | `$x = op-reason --reason "確認 W7: 受け取る" whoami --format json`、許可 | `$x` に JSON の文字が入る |
| W8 | `op-reason --reason "確認 W8: 止める" vault list` の画面が出ている間に Ctrl+C | 窓が消える。画面は拒否する。`end` は `interrupted` |

## まだ分かっていないこと: `--all` の無い `signout` は 1 つのプロセスの session だけを閉じるか

op-reason は呼び出しの終わりに `op.exe signout --all` を打ち、これはほかのプロセスの session (別のタブで手で打った `op`
など) も閉じます。`--all` の無い `signout` が打ったプロセスの session だけを閉じるなら、そちらに替えられます。確かめ方
(Windows PowerShell のタブを 2 つ使う):

1. タブ 1: `op.exe vault list > $null`、許可。続けて `op.exe whoami` → sign in している
2. タブ 2: `op.exe vault list > $null`、許可。続けて `op.exe signout` (`--all` なし)
3. タブ 1: もう一度 `op.exe whoami`。sign in したまま → `signout` はプロセスごと。sign in していない → プロセスごとではない
4. どちらかのタブで `op.exe signout --all` を打ち、1Password をロックする

## 結果の残し方

日付・版 (`op-reason --version`・`op.exe --version`・Windows の build) と、確認ごとに見込みどおりだったか (違ったら何が
起きたか) を書き留めます。見込みと違った結果は issue にしてください。
