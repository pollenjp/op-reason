# 本物の 1Password で op-reason を確かめる

English: [manual-verification.md](manual-verification.md)

CI は偽の `op-reason.exe`・`powershell.exe`・`op.exe` でしか op-reason を流せません (1Password のデスクトップアプリは CI で
動かず、CI の Windows の runner では WSL2 も動きません)。ここの確認には、人と、1Password のアプリと 1Password CLI の入った
Windows と、WSL が要ります。WSL の確認 (1〜14 と R) で 30 分ほどかかり、承認の画面が 15 回ほど出ます。
参照の一覧の確認 (F) は 10 分ほど、SSH の中継の確認 (S) は 40 分ほど足します。

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

## SSH の中継を確かめること

1Password の SSH agent の前に立つ `op-reason.exe --ssh-agent` を確かめます
([2026-11-17-ssh-relay-design.md](superpowers/specs/2026-11-17-ssh-relay-design.md))。CI は偽の agent で中継を流し、
ボタンも押しますが、1Password の画面は出せません。ここは 40 分ほどかかり、1Password の画面が 5〜8 回出ます。GitHub へ
`ssh -T git@github.com` でつなぎます (鍵で認証して `Hi <ユーザー>!` を出して閉じるだけで、GitHub では何も変わりません)。
期待に「書き留める」とある所は、まだ誰も見ていません。見たことを書いてください。最初に流した結果は表の下にあります。

用意:

1. **この PR の Windows の zip**: PR の checks から「Windows exe」の run を開き、artifact の `op-reason-windows` を落とし、
   中の `op-reason-windows.zip` を新しいフォルダーに展開します (例: `$dir = "$env:LOCALAPPDATA\Programs\op-reason-ssh-check"`。
   `gh run download <run の ID> -n op-reason-windows -D $dir` で落として展開してもよい)。`& "$dir\op-reason-windows\op-reason.exe" --version`
   で確かめ、以下では `$exe = "$dir\op-reason-windows\op-reason.exe"` と置きます。
2. **WSL**: 上の「用意」のとおりにこの branch を build し、`ln -sf op-reason target/release/op-reason-ssh` を足します。
   この確かめ用に開いたままにする WSL の shell で (その shell の Linux の `ssh` と `ssh-add` は agent を失います)、中継のパイプを `ssh.exe` に
   渡します: `export SSH_AUTH_SOCK='\\.\pipe\op-reason-ssh-agent' WSLENV="${WSLENV:+$WSLENV:}SSH_AUTH_SOCK"`。S4 の繰り返しの
   script を書きます: `printf '%s\n' 'for i in 1 2 3 4 5; do op-reason-ssh -T git@github.com; done' > /tmp/s4.sh`。
3. 2 つ目の PowerShell の窓で `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'`。ほかにその名前で待ち受けているものが
   無いこと (前に試した中継は止める)。
4. 1Password: **設定 → 開発者 → SSH エージェントを使用する** がオンで、GitHub が知っている鍵があり、**新しいものごとに承認を
   求める** が **アプリケーションおよびターミナルセッション** であること (S10 の後で必ず戻す)。
5. **中継を起こす**: 開いたままにする PowerShell の窓で (親が生きている必要があります) `& $exe --ssh-agent`。
   `the SSH relay listens on \\.\pipe\op-reason-ssh-agent (per-call) and passes requests on to …` と出て止まります。
   Ctrl+C で止まります。確かめは、2 つ目の PowerShell の窓と WSL で打ちます。

| # | すること | 期待 |
| --- | --- | --- |
| S1 | 2 つ目の PowerShell で `$env:OPREASON_SSH_REASON = 'check S1: who am I on GitHub'; ssh -T git@github.com`。**この呼び出しを許可** を押し、1Password の画面を承認する | 右上に op-reason の窓「SSH key: allow this call?」(日本語なら「SSH の鍵: この呼び出しを許可しますか」)。呼び手「Human (terminal) · pwsh.exe」(か powershell.exe)・起点「ssh.exe (pid …)」・理由・コマンド・接続先「git@github.com · host key SHA256:…」・用途「logging in as git」・鍵「<鍵の名前> · SHA256:…」と、ボタン 3 つ (この呼び出しを許可・このターミナルを 60 分許可・拒否)。押すと題が「SSH key: 1Password is asking」に変わり、1Password が答えるまで残る。**画面 1 回**。画面には op-reason.exe でなく、中継を起こした端末のプログラムの名前が出る。続いて `Hi <ユーザー>!` (終了コード 1 は GitHub のもの)。呼び手が「Program: pwsh.exe」なら、その shell の command line (`(Get-CimInstance Win32_Process -Filter "ProcessId=$PID").CommandLine`) を書き留める (対話の shell と見なせなかった。そのとき S2 は窓なしで通る) |
| S2 (C1) | S1 のコマンドを、今度は `SSH_AUTH_SOCK` でなく設定の書き方でもう一度打つ: `Remove-Item Env:SSH_AUTH_SOCK; ssh -o 'IdentityAgent=\\\\.\pipe\op-reason-ssh-agent' -T git@github.com; $env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'`。**この呼び出しを許可** を押す | 窓がまた出る (新しい呼び出し)。**1Password の画面は出ず**、1 秒もかからずに通る。先頭の 2 つの `\` を重ねた `IdentityAgent` が中継に届くことの確かめも兼ねる。ssh が agent につなげないと言ったら書き留める |
| S3 (C1) | WSL で `OPREASON_SSH_REASON='check S3 from WSL' op-reason-ssh -T git@github.com`。**この呼び出しを許可** を押す | 窓に WSL の行 (ディストリビューション)、shell が herdr の中なら (`HERDR_ENV=1`) herdr の行、呼び手「Human (terminal) · <使っている shell>」と理由が出る。**画面なし** |
| S4 | WSL で `OPREASON_SSH_REASON='check S4: five connections' bash /tmp/s4.sh`。**この呼び出しを許可** を 1 回押す | 5 回の接続に窓は 1 つ (起点「bash (pid …) · bash /tmp/s4.sh」)。`Hi <ユーザー>!` が 5 回、画面なし。もう一度打つと、窓がまた 1 つ出る (新しい呼び出し) |
| S5 | S4 を `check S5: deny` で打ち、**拒否** を押す | 5 回とも、押した直後にまとめて失敗する (`Permission denied (publickey)`)。窓はそれ以上出ない |
| S6 (任意) | ansible があれば、鍵の通るホストへの playbook を `ANSIBLE_SSH_EXECUTABLE=op-reason-ssh OPREASON_SSH_REASON='check S6: playbook' ansible-playbook …` で流す。許可し、もう一度流す | タスクとホストの数によらず、1 回の実行に窓は 1 つ。fork が動く間に、ボタンの無い窓と toast がいくつ出るかを書き留める |
| S7 | **S8 を先に流す** (S7 のターミナルの答えの後は、そのターミナルで打つ ssh が窓なしで通り、S8 のコマンドも通ってしまう)。2 つ目の PowerShell で理由なしに (`Remove-Item Env:OPREASON_SSH_REASON`) `ssh -T git@github.com`。**このターミナルを 60 分許可** を押し、もう一度打つ。続けて、Git for Windows を Windows の OpenSSH に向けて (`$env:GIT_SSH = (Get-Command ssh.exe).Source`。Git 自前の ssh はパイプに届かない) `git ls-remote git@github.com:<あなた>/<あなたの repository>.git`。次に PowerShell の新しいタブで (そこでも `SSH_AUTH_SOCK` を置いて) ssh のコマンドを打つ | 理由は「no reason given」(理由なし)。2 回目と `git ls-remote` は窓なしで通る (そのターミナルで打ったコマンド)。新しいタブではまた窓が出る (拒否する) |
| S8 | S7 より前に (か、`SSH_AUTH_SOCK` を置いた PowerShell の新しいタブで)、S1 のコマンドを `check S8: no answer` で打ち、60 秒押さない | 60 秒で窓が閉じ、ssh は失敗する。もう一度打つと、また窓が出る |
| S9 (C4) | `ssh -A <エージェント転送を許すあなたのホスト>` (その呼び出しは許可。S7 の後ならターミナルの答えで通る)、そのホストで `ssh -T git@github.com` を 2 回 | ホストからの署名ごとに窓が出る: 呼び手「unknown (forwarded from a remote host)」、接続先「forwarded from <ホスト> (host key …) · onward to the host with key …」、最下行「this answer covers this signature only」。それぞれ許可する。画面なし |
| S10 (C2) | 中継を止める (Ctrl+C)。1Password の **新しいものごとに承認を求める** を **リクエスト** にする。`& $exe --ssh-agent --mode show-only` を起こし、S1 のコマンドを `check S10: request` で 2 回打ち、画面をそれぞれ承認する | 1 回ごとに、op-reason の窓「SSH key: 1Password is asking」(ボタンなし、最下行「show-only mode: decide in 1Password's prompt …」) と 1Password の画面が一緒に出る: **画面 2 回**。終わったら 1Password を **アプリケーションおよびターミナルセッション** に戻す |
| S11 | show-only の中継のまま、S1 のコマンドを `check S11: default` で 2 回打つ (画面が出たら承認する)。画面も答えも来なければ (1Password の設定を切り替えた直後に 1 回あった)、60 秒の時間切れを待って打ち直す | 2 回目の署名は 0.5 秒もかからずに通り、toast「SSH key: 1Password did not ask」が 1 回だけ出る。`& $exe --log` に「SSH: 1Password did not ask (show-only)」 |
| S12 | 中継を止めて `& $exe --ssh-agent` を起こし直す。1Password をロックする。S1 のコマンドを `check S12: locked` で打ち、**この呼び出しを許可** を押し、1Password のアクセスリクエストに答える (Windows Hello) | アクセスリクエスト 1 つで、1Password のロックの解除と中継の承認が済む (2 つ目の画面は出ない)。それまで op-reason の窓は「SSH key: 1Password is asking」のまま残り、1Password が答えると閉じる |
| S13 (任意) | 端末のプログラム自身の ssh の接続を中継に向け (`~/.ssh/config` のそのホストに `IdentityAgent \\\\.\pipe\op-reason-ssh-agent`)、つないでから、つなぎ直させる | 最初の接続は窓が出る (呼び手「Program: <その exe>」)。つなぎ直しは、そのプログラムが終わるまで窓なしで通る。窓を待つ間にプログラムが何を出すかを書き留める |
| S14 (C3) | WSL の使い捨ての repository で、1Password で commit に署名する設定 (`gpg.format ssh`、`gpg.ssh.program` を `op-ssh-sign-wsl.exe`、`commit.gpgsign true`) にし、鍵を repository に置く: `git config user.signingkey "key::$(ssh-add.exe -L \| grep '<鍵の名前>' \| head -n1)"` (用意の shell では Linux の `ssh-add` に agent が無いので、それに聞く `gpg.ssh.defaultKeyCommand` は鍵を取れない)。続けて `git commit --allow-empty -m 'check S14'` | op-reason の窓は出ず、中継の記録も無い: `op-ssh-sign-wsl.exe` は `SSH_AUTH_SOCK` を使わずに 1Password のアプリと直に話すので、署名は中継の外にある。1Password の画面だけが出て、commit には署名が付く (`git cat-file -p HEAD` に `gpgsig -----BEGIN SSH SIGNATURE-----`) |
| S15 | 中継を止める。PowerShell で `cmd /c start "" /b $exe --ssh-agent` (cmd はすぐ終わり、中継は親なしで残る)。数秒のうちの toast を書き留め、S1 のコマンドを `check S15: no parent` で打って許可する | toast「op-reason's SSH relay has no parent process」か「… lost its parent process」。1Password は画面を出さない: 60 秒ほどで中継が `no-answer` を記録し、ssh は失敗し、toast「SSH key: 1Password did not answer …」が原因つきで出る。この中継は `Stop-Process -Id ((& $exe --ssh-agent --status) -replace '.*pid (\d+).*', '$1')` で止める |
| S16 | `& $exe --log --since 2h`、続けて `& $exe --log --json --since 2h` | `op` の呼び出しの間に、署名 1 つにつき 1 行 (「SSH allowed」「SSH passed」「SSH denied」「SSH timed out」「SSH signed」「SSH no answer from 1Password」)。呼び手と理由つき (passed の行には、その呼び出しの呼び手と理由)。中継そのものの行 (起動・親・1Password が確認しなかった) も出る。JSON には署名した中身も署名そのものも無い |
| S17 (任意) | ディストリビューション自身の起動プログラム (スタートメニューの Ubuntu など) で WSL を開き、用意のとおりに `SSH_AUTH_SOCK` と `WSLENV` を置いて、S3 のコマンドを `check S17: launcher` で打つ | 窓に WSL の行があるかを書き留める。無ければ、窓が出ている間に `ssh.exe` の親を書き留める: `Get-CimInstance Win32_Process -Filter "Name='ssh.exe'" \| ForEach-Object { (Get-Process -Id $_.ParentProcessId).Path }` |

終わったら元に戻します: 中継を止め、1Password の設定を **アプリケーションおよびターミナルセッション** にし、1Password を 1 回
ロックします。

### 2026-11-18 の結果

この PR の op-reason 0.3.0 (Windows は PR の Windows exe の workflow の zip、WSL は branch の release の build)、1Password CLI
2.41.1、1Password のアプリ 8.12.46.29 (Microsoft Store 版)、Windows の OpenSSH 10.4p1、Windows 11 build 26300。1Password は
**アプリケーションおよびターミナルセッション** (S10 だけ **リクエスト**)。中継と確かめの shell は、端末のプログラムの中の
PowerShell 7 と、WSL のディストリビューションの bash で起こし、中継の PowerShell は開いたままにした。上の手順と期待は、実際と
違ったところを直してある。

| # | 結果 | 起きたこと |
| --- | --- | --- |
| S1 | 見込みどおり | 窓の欄とボタン 3 つがそろった。1Password の画面は 1 回で、名前は op-reason.exe でなく端末のプログラムだった (期待には op-reason.exe とあった) |
| S2 | 見込みどおり | `\` を重ねた `IdentityAgent` で中継に届き、画面なしで通った。1・2 回目は、主の画面の右上の窓に気づかず、60.2 秒で時間切れになった |
| S3 | 見込みどおり | WSL の行・呼び手「人 (端末) · bash」・理由。画面なし。shell が herdr の中ではないので、herdr の行は無い |
| S4 | 見込みどおり | 5 回の接続に窓 1 つ (起点 `bash /tmp/s4.sh`)。残り 4 回は同じ decision で 0.1 秒で通った |
| S5 | 見込みどおり | 拒否 1 回で、残り 4 回は同じ decision ですぐ拒否された。押してから 5 秒のうちに 5 回とも `Permission denied` |
| S6 | 飛ばした | ansible が無い |
| S7 | 見込みどおり | 理由なし。ターミナルの答えの後、ssh と `git ls-remote` は窓なしで通った。新しいタブではまた窓が出た (拒否)。Git for Windows を Windows の OpenSSH に向ける必要があった (手順に足した) |
| S8 | 見込みどおり | S7 より前に流した (S7 の後の同じターミナルでは、ターミナルの答えで通る。手順に足した)。60 秒で窓が閉じて ssh が失敗し、次はまた窓が出て (S2 の時間切れでも確かめた)、拒否した |
| S9 | 見込みどおり | エージェント転送を許すホストへのログインは、S7 のターミナルの答えで通った。そのホストからの署名 2 つは、それぞれ窓 (呼び手「分からない (リモートのホストから転送)」・転送元とその先のホストの鍵・この署名だけ) が出て、許可した。画面なし |
| S10 | 見込みどおり | 2 回とも、ボタンの無い窓 (題「SSH の鍵: 1Password が確認しています」と show-only の最下行) と 1Password の画面が一緒に出た: 画面 2 回 |
| S11 | やり直して見込みどおり | 1Password の設定を戻した直後の 1 回目は、画面も答えも 60 秒来なかった。中継は窓を出し続け、時間切れの toast を出してから失敗を返した。次はふつうに画面が出て、承認の後の署名は 0.3 秒と 0.2 秒で通り、記録「SSH: 1Password が確認しなかった (show-only)」と toast は 1 回だけ出た。その toast は、通知センターのほかの toast に埋もれて見逃した |
| S12 | 見込みどおり | アクセスリクエスト 1 つ (Windows Hello) で、1Password のロックの解除と中継の承認が済んだ。それまで窓は「1Password が確認しています」のまま残り、認証の後に閉じた |
| S13 | 飛ばした | 任意 |
| S14 | 中継の外 | commit に署名は付いたが、中継の記録は 0 件で窓も出なかった。`op-ssh-sign-wsl.exe` は `SSH_AUTH_SOCK` を使わずに 1Password のアプリと直に話す。出たのは 1Password の画面だけ。鍵は repository に置く必要があった (手順に足した) |
| S15 | 見込みどおり | `cmd /c start "" /b` で起こすと、中継に親の process が無いという記録と toast が出た。許可しても 1Password は画面を出さず、59.7 秒で中継が `no-answer` を記録して ssh は失敗し、toast「SSH の鍵: 1Password が答えませんでした」に原因が付いた |
| S16 | 見込みどおり | 署名 1 つに 1 行で、見た結果が全部出た: 許可・許可 (ターミナル)・許可 (転送)・通過・拒否・時間切れ・署名・1Password が答えない。通過の行にも、その呼び出しの理由が出る。中継そのものの行も出る。JSON の `ssh-*` の記録 42 件に、署名した中身も署名そのものも無い |
| S17 | 飛ばした | 任意 |

最後に中継を止め、1Password の設定を確かめて、1Password を 1 回ロックした。

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
