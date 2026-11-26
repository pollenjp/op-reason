# 本物の 1Password で op-reason を確かめる

English: [manual-verification.md](manual-verification.md)

CI は偽の `op-reason.exe`・`powershell.exe`・`op.exe` でしか op-reason を流せません (1Password のデスクトップアプリは CI で
動かず、CI の Windows の runner では WSL2 も動きません)。ここの確認には、人と、1Password のアプリと 1Password CLI の入った
Windows と、WSL が要ります。WSL の確認 (1〜14 と R) で 30 分ほどかかり、承認の画面が 15 回ほど出ます。
参照の一覧の確認 (F) は 10 分ほど、SSH の中継の確認 (S) は 1 時間ほど、設定の確認 (G) は 20 分ほど、見た目の確認 (V) は
15 分ほど足します。

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
ボタンも押しますが、1Password の画面は出せません。ここは 1 時間ほどかかり、1Password の画面が 10〜13 回出ます。GitHub へ
`ssh -T git@github.com` でつなぎます (鍵で認証して `Hi <ユーザー>!` を出して閉じるだけで、GitHub では何も変わりません)。
期待に「書き留める」とある所は、まだ誰も見ていません。見たことを書いてください。これまでに流した結果は表の下にあります。

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
   求める** が **アプリケーションおよびターミナルセッション** であること (S10 と S18 の後で必ず戻す)。
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
| S15 | 中継を止める。PowerShell で `cmd /c start "" /b $exe --ssh-agent` (cmd はすぐ終わり、中継は親なしで残る)。数秒のうちの toast を書き留め、S1 のコマンドを `check S15: no parent` で打って許可し、1Password の画面で承認する | toast「op-reason's SSH relay has no parent process」か「… lost its parent process」。1Password がこれからはおそらく中継そのものをアプリと見ること、確認の画面をどこで待たせるかを書いている。許可すると op-reason の窓が前面に出て、1Password の画面がすぐ出る。名前は op-reason (窓が前面に出るまでは、1Password はこの画面をアイコンに伏せたまま見せず、依頼は 60 秒ほどで答えなしに終わっていた: 下の結果)。この中継は `Stop-Process -Id ((& $exe --ssh-agent --status) -replace '.*pid (\d+).*', '$1')` で止める |
| S16 | `& $exe --log --since 2h`、続けて `& $exe --log --json --since 2h` | `op` の呼び出しの間に、署名 1 つにつき 1 行 (「SSH allowed」「SSH passed」「SSH denied」「SSH timed out」「SSH signed」「SSH no answer from 1Password」)。呼び手と理由つき (passed の行には、その呼び出しの呼び手と理由)。中継そのものの行 (起動・親・1Password が確認しなかった) も出る。JSON には署名した中身も署名そのものも無い |
| S17 (任意) | ディストリビューション自身の起動プログラム (スタートメニューの Ubuntu など) で WSL を開き、用意のとおりに `SSH_AUTH_SOCK` と `WSLENV` を置いて、S3 のコマンドを `check S17: launcher` で打つ | 窓に WSL の行があるかを書き留める。無ければ、窓が出ている間に `ssh.exe` の親を書き留める: `Get-CimInstance Win32_Process -Filter "Name='ssh.exe'" \| ForEach-Object { (Get-Process -Id $_.ParentProcessId).Path }` |
| S18 | 1Password に直につなぐ client。用意の 5 のとおりに中継を起こし直す (中継・2 つ目の PowerShell・この確かめで開く 3 つ目の端末は、どれも 1 つの端末のプログラムの中の端末にする)。1Password をロックする。2 つ目の PowerShell で S1 のコマンドを `check S18: through the relay` で打ち、**この呼び出しを許可** を押して 1Password に答える。続けて 3 つ目の端末で、1Password のパイプに直に `ssh -o 'IdentityAgent=\\\\.\pipe\openssh-ssh-agent' -T git@github.com` を打ち、出た画面は拒否する。次に 1Password の **新しいものごとに承認を求める** を **アプリケーション** にし、1Password をロックして、同じ端末で 2 つのコマンドをもう一度打つ (S11 のように、切り替えた直後の依頼に答えが来なければ、時間切れを待って打ち直す)。最後に `& $exe --log --since 30m` | **アプリケーションおよびターミナルセッション** では、直につなぐ client に、op-reason の窓なしで 1Password の画面が出る (名前は端末のプログラム)。拒否すると ssh は `agent refused operation`。**アプリケーション** では、中継を承認したあと、直につなぐ client は画面なしで通る (`Hi <ユーザー>!`)。記録には中継を通した呼び出し 2 つが出て、直につないだ client の行は無い。終わったら 1Password を **アプリケーションおよびターミナルセッション** に戻す |
| S19 | WSL で、その端末を前面にしたまま `OPREASON_SSH_REASON='check S19: toasts' bash /tmp/s4.sh` (1 つの呼び出しで 5 回の接続) を打ち、**この呼び出しを許可** を 1 回押す | 最初の署名には窓が出て、トーストは出ない。残りの 4 回は窓なしで通り、WSL の端末のある画面の左上 (作業領域の端から 12 DIP) の 1 枚のカードに出る: `通過 · 人`、来るたびに緑の ×2〜×4、2 行目 `ssh.exe … → git@github.com` (接続先が切れないようにコマンドを「…」で切る)、3 行目 `この呼び出しの許可 · check S19: toasts`。カードは濃さ 90% で出て、地も字も線も同じように後ろの字がうっすら透けて見える。署名のたびにカードのまわりに緑の輪が光り (カードは 90% のまま)、下の細い緑の線が縮む。最後の署名から 4 秒ほどで、1 秒かからずに薄れて消える。出ている間に打ったキーは端末に届き、カードの上のクリックは下の窓に届く。通知センター (Win+N) には何も増えない |
| S20 | 新しい PowerShell のタブで (そのタブでは理由を置かない) `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; ssh -T git@github.com` を打ち、**このターミナルを 60 分許可** を押す。続けてそのタブで、4 つのコマンドを一度に: `1..4 \| ForEach-Object { Start-Process -NoNewWindow ssh.exe '-T git@github.com' }`。カードが消えたら、10 個を一度に: `1..10 \| ForEach-Object { Start-Process -NoNewWindow ssh.exe '-T git@github.com' }` | 4 つのコマンドは、ターミナルの答えが覆う 4 つの呼び出し: カードが 4 回出て、新しいものが一番上、ほかは下へずれる。4 枚目が来たとき 1 枚目がすぐ消え (同時に 3 枚まで)、3 枚目の下に小さく `+1` が出て、1 枚目の署名から 4 秒ほどで消える。どれも `通過 · 人` と `ターミナルの許可 · 理由なし` で、それぞれの署名から 4 秒で薄れる。10 個のコマンドでは、カード 3 枚と 3 枚目の下の `+7`。消したカードの 4 秒が過ぎるたびに数が減り (少しずつずれて来たため)、3 枚が薄れる前に消える |
| S21 | Windows のアニメーション効果を切り (設定 → アクセシビリティ → 視覚効果 → アニメーション効果)、S20 のタブで 4 つのコマンドをもう一度打ち、効果を戻す | カードは濃さ 90% でその場に出て、ずれたり薄れたり光ったりせずに動いて消える。`+1` もその場で出て消える |
| S22 (任意・画面が 2 つ) | 用意の WSL の端末を主でない画面へ移し、中を押す。`OPREASON_SSH_REASON='check S22: the other screen' bash /tmp/s4.sh` を打って **この呼び出しを許可** を押し、続けてその端末で `op --reason "check S22: op window" vault list >/dev/null` を打ち、承認する | op-reason の窓 (中継のものと `op` の呼び出しのもの) はその画面の右上に、後の 4 回の署名のトーストはその画面の左上に出る。`op` の呼び出しの 1Password の画面がどの画面に出るかを書き留める (設計 2026-11-18 §5: その画面でなければ、画面の選び方を相談する) |
| S23 | 新しい PowerShell のタブで `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; $env:OPREASON_SSH_REASON = 'check S23: notification'; ssh -T git@github.com`。窓が出ている間に右下を見て、**拒否** を押し、通知センター (Win+N) を開く。続けて WSL で `op --reason "check S23: op" vault list >/dev/null` を打ち、承認して、もう一度通知センターを開く | Windows の通知「SSH の鍵: この呼び出しを許可しますか」(英語なら「SSH key: allow this call?」) が窓と一緒に出て、押すと窓と一緒に消える: 通知センターにも残らない。`op` の呼び出しの通知も、窓が閉じると消える。前の確かめから残るのは、S8 の時間切れの通知と 1 回だけの知らせ (S11・S15) |
| S24 | 新しい PowerShell のタブで `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; $env:OPREASON_SSH_REASON = 'check S24: unanswered'; ssh -T git@github.com` を打ち、60 秒押さずに待ってから、通知センターを開く | 60 秒で窓が閉じて ssh が失敗し、その通知は答えなかった依頼の印として通知センターに残る |

終わったら元に戻します: 中継を止め、1Password の設定を **アプリケーションおよびターミナルセッション** にし、1Password を 1 回
ロックします。

### 許可の後に前面に出る窓 (S25〜S31)

**この呼び出しを許可**・**このターミナルを許可** を押すと、op-reason の窓は 1Password が答えるまで前面に出て、1Password が
確認の画面をすぐ出せるようにします
([2026-11-22-front-after-allow-design.md](superpowers/specs/2026-11-22-front-after-allow-design.md))。1Password は、前面に
見えないアプリの確認の画面を、タスクバーの右のアイコンに伏せます (アイコンに点が付き、メニューに **SSHリクエストを待機中**
が出る)。タスクから起こした中継では、そのアプリは op-reason です。S25〜S28 と S31 はその中継で確かめます。用意の中継を
止め (Ctrl+C)、PowerShell で trigger の無いタスク (起こしたときだけ動く) を置いて起こします:

```powershell
$a = New-ScheduledTaskAction -Execute $exe -Argument '--ssh-agent'
$s = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -Hidden
Register-ScheduledTask -TaskName op-reason-ssh-check -Action $a -Settings $s | Out-Null
Start-ScheduledTask -TaskName op-reason-ssh-check
& $exe --ssh-agent --status
```

S25 の前に 1Password をロックして解き、まだ何も承認していない状態にします。op-reason の窓が応答しなくなったら (中が白く、
題に「応答なし」)、ほかの何より先に `& $exe --log --since 10m` を打ち、「SSH: op-reason の窓が止まっている (<段>)」の行を
書き留めます。窓のスレッドが止まっている段です。

| # | すること | 期待 |
| --- | --- | --- |
| S25 | 2 つ目の PowerShell を前面にしたまま `$env:OPREASON_SSH_REASON = 'check S25: from a task'; ssh -T git@github.com`。op-reason の窓が出ている間に字をいくつか打ち (Enter は押さない)、**この呼び出しを許可** を押し、1Password の画面で承認する | 窓はフォーカスを取らずに出る: 打った字は端末に入る。押すと窓の題の帯が前面の色になり (フォーカスを持つ)、1Password の画面がすぐ出る。アイコンの点は要らない。名前は端末のプログラムでなく op-reason。承認すると `Hi <ユーザー>!` が出て、端末にフォーカスが戻る (打って確かめる)。戻らなければ、どの窓が持っているかを書き留める |
| S26 | S25 のコマンドを `check S26: again` で打ち、許可する | 窓がまた出て (新しい呼び出し)、押すと前面に出る。1Password の画面は出ずにすぐ通り、端末にフォーカスがすぐ戻る |
| S27 | S25 のコマンドを `check S27: deny` で打ち、**拒否** を押す | 窓はフォーカスを取らない (題の帯は前面でない色のまま)。ssh はすぐ失敗する |
| S28 | タスクの中継を起こし直す: `Stop-Process -Id ((& $exe --ssh-agent --status) -replace '.*pid (\d+).*', '$1'); Start-ScheduledTask -TaskName op-reason-ssh-check`。S25 のコマンドを `check S28: a new relay` で打ち、許可して承認する | 1Password がもう一度聞く (中継の新しい process は承認し直しが要る)。画面はすぐ出て、名前は op-reason |
| S29 | タスクの中継を止め (S28 の `Stop-Process`)、用意の 5 のように端末で中継を起こす。同じ端末のプログラムの別の端末で、そのプログラムを前面にしたまま、S1 のコマンドを `check S29: started in a terminal` で打ち、許可して承認する | 押しても op-reason の窓はフォーカスを取らない (中継を起こしたプログラムが前面にある)。1Password の画面はすぐ出て、名前は S1 と同じくその端末のプログラム |
| S30 (任意) | S29 の中継のまま、1Password をロックして解く (解かないと、S29 の承認で画面なしに通る)。S1 のコマンドを `check S30: another program in front` で打つ。許可を押す前に、タスクバーの何もない所 (かほかのプログラムの窓) を 1 回押してから、許可を押す | 押すと op-reason の窓が前面に出て、1Password の画面がすぐ出る。名前は端末のプログラム: 1Password は、アプリと見る端末のプログラムの下の process である op-reason の窓を数える (2026-11-22 に確かめた)。アイコンに伏せたら書き留める |
| S31 | S29 の中継を止め (Ctrl+C)、タスクに show-only の中継を起こさせる: `Set-ScheduledTask -TaskName op-reason-ssh-check -Action (New-ScheduledTaskAction -Execute $exe -Argument '--ssh-agent --mode show-only') \| Out-Null; Start-ScheduledTask -TaskName op-reason-ssh-check`。1Password をロックして解く。S1 のコマンドを `check S31: show-only from a task` で打ち、60 秒そのままにする。もう一度打ち、今度は 1Password のアイコンから画面を開いて承認する | op-reason の窓「SSH の鍵: 1Password が確認しています」はフォーカスを取らず、最後の行は「1Password の画面が出ないとき: タスクバーの右の 1Password のアイコン (隠れていれば ^ の中) →「SSHリクエストを待機中」」。アイコンに点が付く。そのままにすると 60 秒ほどで依頼が閉じて ssh が失敗し、toast「SSH の鍵: 1Password が答えませんでした」は、1Password がそういう画面をアイコン (SSHリクエストを待機中) に伏せることを書く (中継にタスクが要る、とは書かない)。2 回目はアイコンから開いた画面の名前が op-reason で、`Hi <ユーザー>!` が出る |

終わったら元に戻します: タスクを消し、まだ動いていれば中継を止め、1Password を 1 回ロックします。

```powershell
Stop-ScheduledTask -TaskName op-reason-ssh-check
Unregister-ScheduledTask -TaskName op-reason-ssh-check -Confirm:$false
if ((& $exe --ssh-agent --status) -match 'pid (\d+)') { Stop-Process -Id $Matches[1] }
```

### 許可の後の案内 (S32〜S36)

**許可** を押した後、op-reason の窓は 1Password の画面が出るかを見張ります
([2026-11-26-prompt-guide-design.md](superpowers/specs/2026-11-26-prompt-guide-design.md))。1Password の窓が出たら
「1Password の画面で承認してください」と出し、2 秒たっても出なければ、窓が光って約 60 秒を数え、通知領域の横の小窓が
1Password のアイコン (SSHリクエストを待機中) を指します。きっかけ: 1Password を起こし直した直後でロックしていたときに、
op-reason の窓を押したのに、1Password は確認の画面をアイコンの後ろに伏せました。S25 と同じく、スケジュールされたタスクが
起こした per-call の中継が要ります。S29 の中継を止め (Ctrl+C)、S25〜S31 のタスクを消したあとで、もう一度登録して起こします:

```powershell
$a = New-ScheduledTaskAction -Execute $exe -Argument '--ssh-agent'
$s = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -Hidden
Register-ScheduledTask -TaskName op-reason-ssh-check -Action $a -Settings $s | Out-Null
Start-ScheduledTask -TaskName op-reason-ssh-check
& $exe --ssh-agent --status
```

署名の最後の記録: `Get-Content "$env:LOCALAPPDATA\op-reason\ssh-log.jsonl" | Select-Object -Last 1`。

| # | すること | 期待 |
| --- | --- | --- |
| S32 | 1Password を終了し (アイコンのメニュー → 終了)、起こし直してロックのままにする。2 つ目の PowerShell で `$env:OPREASON_SSH_REASON = 'check S32: locked'; ssh -T git@github.com`。**この呼び出しを許可** を押し、5 秒は何もしない | 何が出たかを控える: 前面に 1Password のロック解除か確認の画面 (そのとき op-reason の窓は「1Password の画面で承認してください」と、ダイアログの略図を出す)、または押してから 2 秒以内に、光った op-reason の窓 (黄色い枠、「1Password の画面が前に出ていません」、残り秒) と、通知領域の横の、矢印でそこを指す小窓。そのあと小窓のとおりに 1Password のアイコン (隠れていれば ^ の中) から画面を開き、ロックを解いて承認する: `Hi <user>!` が出て、小窓は窓と一緒に消える。記録の `prompt` (`front`・`seen`・`hint`) を控える |
| S33 | 1Password をロックして解く (中継をもう一度聞く)。S32 のコマンドを `check S33: unlocked` で。許可を押し、承認する前に op-reason の窓と、1Password の画面が出た場所を見る | 1Password の画面がすぐ出る。op-reason の窓は「1Password の画面で承認してください」と略図を出し、枠も小窓も出ない。画面が、帯の言うとおり画面の中央に出たかを控える。記録は `"seen"` が 2 未満、`"hint":null` |
| S34 | 1Password を終了して起こし直し、ロックのままにする。S32 のコマンドを `check S34: left alone` で。許可を押し、70 秒何も開かない | 小窓と光った窓が約 58 から数え、1Password が依頼を閉じると (押してから約 60 秒)、窓と小窓が消え、ssh は失敗し、「SSH の鍵: 1Password が答えませんでした」のトーストが出る。記録は `no-answer`、`"seen":null`、`"hint"` あり |
| S35 | S33 をもう一度。ただし許可を押した直後にほかの窓 (デスクトップ) を押し、1Password の画面が後ろに出るかもしれないようにする | 画面がそれでも前面に出れば S33 のとおり。出なければ 2 秒以内に小窓。1Password が何を出したか (アイコンの点、タスクバーの窓) を控える |
| S36 (任意) | Windows が許せば、タスクバーを上 (か横) に移し、S34 を 10 秒だけ流す | 小窓はそこの通知領域の横に、矢印でそこを指して、画面の中に出る |

終わったら S31 の後と同じく元に戻します (上のブロック): タスクを消し、まだ動いていれば中継を止め、1Password を 1 回ロックします。

### Claude Code の PowerShell の呼び出しと、プロセスの木 (S37〜S41)

Windows の Claude Code の PowerShell の tool は、スクリプトを起動の行 (`chcp.com 65001` の後に PowerShell を起こす
`cmd.exe`。スクリプトは環境変数で渡す) で走らせ、その行にコマンドはありません。窓はその起動の行を見分け、木の ssh の
process の段に ssh のコマンドの枠を出し、ssh ごとに聞きます
([2026-11-26-modern-window-design.md](superpowers/specs/2026-11-26-modern-window-design.md) の §4.7 の規則 3 と §13、
[2026-11-27-relay-process-tree-design.md](superpowers/specs/2026-11-27-relay-process-tree-design.md))。
`\\.\pipe\op-reason-ssh-agent` の per-call の中継 (用意の 5 か、S32 のタスク) と、PowerShell の tool のある Windows の
Claude Code が要ります。S40 は `op-reason-ssh` のある WSL が要ります。

| # | すること | 期待 |
| --- | --- | --- |
| S37 | Claude Code に、PowerShell の tool の 1 回の呼び出しで `$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'; $env:OPREASON_SSH_REASON = 'check S37: PowerShell tool'; ssh -T git@github.com` を打たせる。**この ssh を許可** を押し、1Password の画面で承認する | 題は「SSH の鍵: この ssh を許可しますか」。「プロセスの木」に、Claude Code (`claude.exe · pid …`、紫の点) → `cmd.exe` (pid …。「Claude Code の PowerShell」の札と「起点」の印) → `pwsh.exe` (か `powershell.exe`) → `ssh.exe` が並び、`ssh.exe` の下に枠 `ssh.exe -T git@github.com` が出る。点線の囲み「この呼び出し」が `cmd.exe` から下を、実線の囲み「この ssh」が `ssh.exe` とその枠を囲む。ボタンは **この ssh を許可** (塗り)・**この呼び出しを許可**・**拒否**。残り秒の横は「許可は、選んだ枠 (この ssh か、この呼び出し) が終わるまで効きます (長くても 60 分)」。`Hi <user>!` が Claude Code に返る。記録は `"answer":"allow-ssh"` で、`call.root.cmd` に起動の行が入る。起点の段の下に `chcp.com 65001 >nul &` を含む枠が出たら、`claude --version` を控える (起動の形が変わったかもしれない) |
| S38 | S37 と同じで、1 回の呼び出しに ssh を 2 つ入れる: `…; ssh -T git@github.com; ssh -T git@github.com`。1 つ目の窓で **この ssh を許可**、続けて 2 つ目の窓でも押す | 1 つ目が署名した後、2 つ目の ssh はそれぞれの窓で聞く (木の `ssh.exe` の PID が違う)。どちらも `Hi <user>!`。記録は `"answer":"allow-ssh"` の `allowed` が 2 つで、`passed` は無い |
| S39 | S38 をもう一度。1 つ目の窓で **この呼び出しを許可** を押す | 2 つ目の ssh は窓なしで署名し、左上のトーストに「通過 · Claude Code」、ssh のコマンド、「この呼び出しの許可」が出る。2 つ目の記録は `passed` で `"by":"allow"`、`decision` は 1 つ目の記録の `id`。`op-reason.exe --log` に両方が出る |
| S40 | WSL から Claude Code に、Bash の tool の 1 回の呼び出しで `OPREASON_SSH_REASON='check S40: WSL tree' ssh -T git@github.com` を打たせる。窓を見てから **この呼び出しを許可** | 木は Claude Code (`pid …`、Linux の PID) → 起点 (`bash`、「起点」の印、枠に `ssh -T git@github.com`、行の残りは枠の下に小さく) →「… WSL の中は見えない」→ `ssh.exe` (Windows の PID。下に ssh のコマンドを小さく)。実線の囲み「この呼び出し」が起点から下を囲み、ボタンは今までどおり **この呼び出しを許可**・**拒否** |
| S41 | Windows Terminal (PowerShell) で `$env:OPREASON_SSH_REASON = 'check S41: terminal tree'; ssh -T git@github.com` を打つ。暗いテーマで窓を見て、開いたまま Windows を明るいテーマにし (設定 → 個人用設定 → 色)、**拒否** | 木は 人 (端末) (`pwsh.exe · pid …`、緑の点) → `ssh.exe` (「起点」の印、枠にコマンド)。実線の囲み「この呼び出し」が `ssh.exe` を、点線の囲み「このターミナル」が shell から下を囲む。ボタンは **この呼び出しを許可**・**このターミナルを 60 分許可**・**拒否**。木の線・囲み・字が、どちらのテーマでも読める |

### 2026-11-18 の結果

この PR の op-reason 0.3.0 (Windows は PR の Windows exe の workflow の zip、WSL は branch の release の build)、1Password CLI
2.41.1、1Password のアプリ 8.12.46.29 (Microsoft Store 版)、Windows の OpenSSH 10.4p1、Windows 11 build 26300。1Password は
**アプリケーションおよびターミナルセッション** (S10 は **リクエスト**、S18 の後半は **アプリケーション**。S18 はほかの後で
足して流した)。中継と確かめの shell は、端末のプログラムの中の PowerShell 7 と、WSL のディストリビューションの bash で起こし、
中継の PowerShell は開いたままにした。上の手順と期待は、実際と違ったところを直してある。

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
| S18 | 「アプリケーション」では確認なし | 最初の実行の後で足し、同じ端末のプログラムの 3 つの端末 (中継・中継を通す呼び出し・直につなぐ client) で流した。「アプリケーションおよびターミナルセッション」では、ロックから中継を許可・承認したあとも、直につなぐ client に op-reason の窓なしで 1Password の画面が出て、拒否した (`agent refused operation`)。「アプリケーション」では、同じ手順のあと、画面なしで 2 秒で通り、中継の記録にその行は無かった。1Password の画面の名前は、中継の依頼も直の依頼も端末のプログラムだった。README は、そういう client を 1Password にとって新しいアプリと書いていた (いつ確認が出るかを書き直した) |

最後に中継を止め、1Password の設定を確かめて、1Password を 1 回ロックした。

### 2026-11-19・2026-11-20 の結果

S19〜S24 を足した PR の op-reason 0.4.0 (Windows は PR の Windows exe の workflow の zip、WSL は branch の release の build) と、
実際の 1Password のアプリで、用意のとおりに流した。流したのは、その PR の 3 つの build:

- 2026-11-19、最初の build (head `7bfa01c`。カードは不透明で、`+N` は無い): S19・S20・S22・S23。S19 の後にユーザーが、
  後ろの字が透けるカード (濃さ 70%) を頼み、S20 の後に、押し出したカードの数 (`+N`) を頼んだ。S21 は、この 2 つを入れた
  build で見ることにした
- 2026-11-20、2 つを入れた 2 つ目の build (head `d7dd317`。濃さ 70% と `+N`): S24、S19・S20 のやり直し、S21。S19 で
  カードが薄すぎたので、濃さを 90% にした
- 2026-11-20、濃さ 90% の 3 つ目の build (head `3e8c62e`): S19 の見直し

S20 と S21 は 3 つ目の build にも当てはまる (`+N` と動きなしは濃さで変わらない)。

| # | 結果 | 起きたこと |
| --- | --- | --- |
| S19 | 見込みどおり (3 つ目の build) | 最初の build (2026-11-19): 窓は前面の窓のある画面 (主でない画面) の右上に 1 回出て、1Password の画面も同じ画面に出た (設計 2026-11-18 §5)。後の署名はその画面の左上の 1 枚のカードに出て (×4)、最後の署名から約 4.5 秒で消えた。見込みどおりだが、カードの後ろの字が読めないので、ユーザーが濃さ 70% を頼んだ。2 つ目の build、濃さ 70% (2026-11-20): カードは左上に 1 枚出て、最後の署名から 4.6 秒で消えたが、薄すぎた (手順の期待は 90% に直した)。カードが出ていた 9.8 秒で、中継の CPU は 0.36 秒 (1 コアの 3.7%)。3 つ目の build、濃さ 90% (2026-11-20): 5 回の接続が 14 秒で通った。窓は 1 回で、前面の窓のある画面の右上に出て、許可の 1 秒後に 1 回目が通った。後の署名は 1 枚のカードに ×2〜×4 で出て、署名のたびに輪が光った。カードの字が読めて、後ろも透けた |
| S20 | 見込みどおり (2 つ目の build) | 4 つ一度に: カード 3 枚と `+1`。10 一度に: `+7` (来るにつれて `+5`、`+7`)。最初のコマンドの窓は前面の窓のある画面の右上に出て、**このターミナルを 60 分許可** で答えた。カードが出ている間の中継の CPU は 1 コアの 3.8%。最初の build でも、4 つ一度に打つとカードは 3 枚で、そのときの見込みどおりだった |
| S21 | 見込みどおり (2 つ目の build) | アニメーション効果を切ると、カードはずれず薄れずに出て消えた。カードが出ていた 3.8 秒の中継の CPU は 0 |
| S22 | 見込みどおり (最初の build) | op-reason の窓 (中継のものと `op` の呼び出しのもの) は主でない画面の右上に、トーストはその画面の左上に出た |
| S23 | 見込みどおり (最初の build) | 通知は窓と一緒に出て、押すと窓と一緒に消えた。通知センターには、それも `op` の呼び出しのものも残らなかった |
| S24 | 見込みどおり (2 つ目の build) | 前面の窓が無い (デスクトップ) ので、窓は主の画面の右上に出た。60 秒で窓が閉じて ssh が失敗し、その通知は通知センターに残った |

### 2026-11-22 の結果

S25〜S31 を足した PR の op-reason 0.5.0 (Windows は PR の Windows exe の workflow の zip) と、実際の 1Password のアプリ
(**アプリケーションおよびターミナルセッション**) で、変更を頼んだ人が流した。手順をこなし、前面の窓と op-reason の窓を
0.2 秒ごとに書き留める script を使った。タスクの中継は自分だけのパイプで動かし、タスクにはログオンの trigger と 1 分ごとに
繰り返す trigger を付けた。S29 と S30 の中継は端末のプログラムで起こした。流したのは、その PR の 3 つの build:

- 最初の build (head `f295109`): タスクの中継の最初の窓が、描かれる前に応答しなくなった (中が白く、「応答なし」)。2 回とも
  そうで、先へ進めなかった。中継の窓は GPU を使わずに描くようにした
  ([設計](superpowers/specs/2026-11-22-front-after-allow-design.md) §12)
- 2 つ目の build (head `3f8f0a0`): S25 の窓は 60 秒が尽きるまでクリックを待ち、クリックは来なかった (script が窓を
  見つけられなかった)
- 3 つ目の build (head `14c0724`。中継の窓は窓の DC に描く): S25・S26・S28・S29・S30

| # | 結果 | 起きたこと |
| --- | --- | --- |
| S25 | 見込みどおり (3 つ目の build) | 窓は ssh の始まりから 1.5 秒で、端末のプログラムから前面を取らずに出た。許可を押すとすぐ前面に出て、その 0.5 秒後に 1Password の画面が出た。名前は op-reason で、アイコンを開く手は要らなかった。承認 (Windows Hello) の後 `Hi <ユーザー>!` が出て、前面は端末のプログラムに戻った |
| S26 | 見込みどおり (3 つ目の build) | 1Password の画面は出なかった。窓は押してすぐ閉じ (前面に出たところは見えないほど短い)、前面は端末のプログラムのままだった。押す前、1Password のアイコンに点は無かった |
| S27 | 流していない | |
| S28 | 見込みどおり (3 つ目の build) | `Stop-Process` で止めた中継は、48 秒後にタスクの 1 分ごとに繰り返す trigger で起き直った (`Start-ScheduledTask` は使っていない)。1Password はもう一度聞いた (ロックして解いてもいた)。画面は押してから 0.4 秒で出て、名前は op-reason。押す前、アイコンに点は無かった |
| S29 | 見込みどおり (3 つ目の build) | 端末のプログラムが前面のまま、押しても窓は前面に出なかった。1Password の画面は 0.6 秒後に出て、名前は端末のプログラム。前面はそのプログラムのままだった |
| S30 | 画面はすぐ出た (3 つ目の build) | 1 回目は画面なしに通った: 1Password が、その中継への S29 の承認を持っていた (手順で先にロックするようにした)。端末のプログラムで中継を起こし直し、1Password をロックして解き、タスクバーを押してから許可を押すと、窓が前面に出て、0.4 秒後に 1Password の画面が出た。名前は端末のプログラム。終わると、前面はタスクバーを押す前の窓に戻った |
| S31 | 流していない | |

3 つ目の build では、応答しなくなった窓は無く (窓は 6 回)、押す前に 1Password の画面も点も出なかった。窓と一緒に出る通知の
差出人は「Windows PowerShell」 (op-reason はその AppUserModelID を借りている)。終わりにタスクを消し、中継を止めた。

## 設定を確かめること (G1〜G19)

`op-reason --settings` ([2026-11-23-settings-design.md](superpowers/specs/2026-11-23-settings-design.md)、英語)。CI は
GitHub の runner (管理者として動く) でテスト用の名前のタスクを作り、`wsl.exe` は偽物を使います。ここで確かめるのは、
管理者でないユーザー・本物のログオン・本物の WSL と 1Password です。約 30 分で、ログオフが 2 回、1Password の確認が 3 回です。

用意:

1. この pull request の Windows の zip を、SSH の中継の用意と同じように展開します。以下 `$exe`。どのコマンドも
   **管理者でない** PowerShell で、`$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'` を入れて打ちます。
2. 中継が動いていないこと: `& $exe --ssh-agent --status` が 1 で終わる。**ほかのタスクが中継を起こしているときは**
   (自分のタスクや dotfiles のタスク)、先にそれを無効にして中継を止め、確かめ終えたら戻します:
   `Disable-ScheduledTask -TaskPath '<そのフォルダ>' -TaskName '<その名前>'` と
   `Stop-ScheduledTask -TaskPath '<そのフォルダ>' -TaskName '<その名前>'` (あとで `Enable-ScheduledTask` と
   `Start-ScheduledTask`)。
3. `%LOCALAPPDATA%\op-reason\settings.json` があれば写しを取っておきます。
4. **skill**: `%USERPROFILE%\.claude\skills` (や WSL の distro の `~/.claude/skills`) に op-reason の手引きを置きたくない
   ときは、G9 を選んだフォルダ (`skill.folder.$env:TEMP\op-reason-skill-check`) で確かめ、G10 は飛ばします。

| # | すること | 期待すること |
| --- | --- | --- |
| G1 | `& $exe --settings list` | 設定ごとに 1 行: `relay.autostart off`・`relay.running off`・WSL の distro ごとの行 (動いていないものは「見ていません」)。最後に記録のフォルダとアカウント。ほかのタスクが中継を起こすなら (用意 2)、`relay.autostart` の行がそれを名指しする |
| G2 | `& $exe --settings set relay.autostart on` | `relay.autostart: off → on`。そのあと `& $exe --ssh-agent --status` が 0 で終わる。タスク スケジューラー (**表示 → 隠しタスクの表示**) のルートに `op-reason-ssh-relay-<あなたの SID>`: あなたのログオン時と 1 分ごと。「新しいインスタンスを開始しない」。あなたのユーザーで「ユーザーがログオンしているときのみ実行する」、最上位の特権ではない |
| G3 | `& $exe --settings check relay.autostart=on relay.running=on --json; $LASTEXITCODE` | `"same":true`、そして 0 |
| G4 | 1Password をロックして解除する。`$env:OPREASON_SSH_REASON = 'check G4'; ssh -T git@github.com`。**この呼び出しを許可**。1Password で承認 | S25 と同じ: 押したときに op-reason の窓が前面に出て、1Password の画面がすぐ出る (名前は op-reason)。`Hi <user>!` |
| G5 | `$env:OPREASON_SSH_REASON = 'check G5'; ssh -T git@github.com`。op-reason の窓が待っている間に、別の PowerShell で `& $exe --settings relay stop; $LASTEXITCODE`。そのあと **この呼び出しを許可** | `relay.running: not changed (off): the relay has a window open, …` (日本語なら「変えていません」)、そして 1。中継はそのまま動き、許可が通る |
| G6 | `& $exe --settings relay stop`。2 分待つ。`& $exe --ssh-agent --status` | 2 分後も 1: 1 分ごとの trigger は止めた中継を起こさない。`& $exe --settings get relay.running --json` が `task-off`。`& $exe --log --since 10m` に「SSH の中継を op-reason --settings が止めた」と、あなたが打った `settings` の行 |
| G7 | ログオフしてログオンし直す。`& $exe --ssh-agent --status`。そのあと `& $exe --settings relay start` | ログオンし直しても止まったまま。`relay start` で 15 秒以内に答える |
| G8 | `& $exe --settings set relay.mode show-only`。そのあと G4 と同じ ssh | `relay.mode: per-call → show-only` と、中継を起こし直した行。`get relay.mode --json` に `relay-differs` が無い。署名ごとに 1Password が確認し (show-only)、op-reason の窓が誰がなぜかを出す。戻すには `set relay.mode default` |
| G9 | `& $exe --settings set skill.windows on` (か用意 4 の選んだフォルダ)。出た `SKILL.md` を開く。Windows の Claude Code で `/skills` | 手引きの閉じる `---` の次の行に `<!-- placed by op-reason <版>: …`。Claude Code の一覧に op-reason の skill。`set skill.windows off` で消える (空の `op-reason` フォルダも) |
| G10 | WSL で、この branch の `op-reason` を使って `op-reason --settings set "skill.wsl.$WSL_DISTRO_NAME" on`、そのあと `off` | 印の付いた `~/.claude/skills/op-reason/SKILL.md` ができ、そのあと消える。`~/.claude/skills` が symbolic link (home-manager) なら断り、何も書かない |
| G11 | `& $exe --settings` | プロンプトがすぐに戻り、設定の窓が `lang` の言語で前面に開く。読んでいる間は **読み直す** が灰色。中継の行は「動いています · pid … · per-call · op-reason のタスクが起こしたもの」。もう一度 `& $exe --settings` を打つと窓が前に出る。Explorer で `op-reason.exe` をダブルクリックしても同じ (Windows 11 24H2 より前はコンソールが一瞬出る。N6) |
| G12 | 窓で **日本語** を選び、そのあと **English** に戻す | 窓がすぐに日本語になり、そして英語に戻る。間に `& $exe --settings get lang` を打つと `ja` |
| G13 | 窓で、Windows の行 (か用意 4 の選んだフォルダ) に **置く・更新する**、そのあと **消す**。動いていない WSL の distro に **確かめる** | 行の状態が変わる (「置いてあります (<版>)」・「置いていません」)。確かめた distro が起き、その行が読まれる。`& $exe --log --since 10m` に窓からの `settings` の行。用意 4 の選んだフォルダで確かめるときは、代わりに **フォルダを足す…** を使う。足すと同時に置くので、行はすぐ「置いてあります (<版>)」になり、**置く・更新する** は灰色のまま (置くものが無い)。**消す** で消え、行もなくなる (フォルダを忘れる) |
| G14 | 窓でモードを `show-only` にする。**適用** を押し、問いに OK。そのあと G4 と同じ ssh | 適用の横の注記が、適用するまで中継は別の値で動くと言う。適用の後、中継の行が `show-only` になり、1Password が署名を確認する。モードを戻して、もう一度 **適用** |
| G15 | `& $exe --settings set start-menu on`。Windows キーを押して `op-reason` と打つ | スタートメニューが **op-reason** を見つけ、設定の窓が開く (Windows 11 24H2 より前は、コンソールがタスクバーに一瞬だけ出る。N7)。`set start-menu off` で消える |
| G16 | 通知領域 (Windows 11 では `^` の中) の中継の鍵 | ツールチップは「op-reason の SSH の中継 · per-call」。クリックで設定の窓が開く。右クリックのメニューに中継の行・**op-reason の設定を開く**・**記録のフォルダを開く**・**中継を起こし直す**・**中継を止める**・**このアイコンを隠す**。**中継を起こし直す** で、15 秒以内に別の process ID の中継が答える (`& $exe --ssh-agent --status` がそれを出す)。**このアイコンを隠す** でアイコンがすぐに消え、`set relay.tray-icon on` で戻る |
| G17 | Explorer を起こし直す (タスク マネージャー → **エクスプローラー** → **再起動**) | 数秒で中継のアイコンが戻る |
| G18 | 中継が動いている (G2 のタスク) まま、ログオフしてログオンし直す | ログオンの後、通知領域に中継のアイコンがある (タスクがタスクバーより先に中継を起こすことがある) |
| G19 | `& $exe --settings set relay.autostart off`。用意 3 の `settings.json` を戻す。用意 2 のほかのタスクを有効に戻す | タスクが消え、op-reason のタスクの中継は答えず、そのアイコンも消える |

## コンソールの窓を確かめること (N1〜N11)

端末の外から `op-reason.exe` を起こしてもコンソールの窓が出ないこと
([2026-11-24-relay-no-console-window-design.md](superpowers/specs/2026-11-24-relay-no-console-window-design.md)、英語)。CI は
GitHub の runner (Windows Server 2025) でコンソールの窓を見張り、zip の exe を Windows Server 2022 で起こします。ここで
確かめるのは、本物のログオン・既定の端末の Windows Terminal・本物の WSL と 1Password です。約 15 分で、ログオフが 1 回、
1Password の確認が 4 回です。

用意:

1. Windows 11 24H2 以降: `winver` が build 26100 以上を出す。(それより前の Windows では、N1〜N4 と N6〜N7 で今までどおり
   コンソールの窓が一瞬出て、`start` の記録は `console: own` になります。)
2. この pull request の Windows の zip を、SSH の中継の用意と同じように展開します。以下 `$exe`。どのコマンドも
   **管理者でない** PowerShell で、`$env:SSH_AUTH_SOCK = '\\.\pipe\op-reason-ssh-agent'` を入れて打ちます。
3. 中継が動いていないこと: `& $exe --ssh-agent --status` が 1 で終わる。**ほかのタスクが中継を起こしているときは**
   (自分のタスクや dotfiles のタスク)、設定を確かめることの用意 2 と同じく、先にそれを無効にして中継を止め、N11 で
   戻します。そのタスクの中継は、この pull request の exe ではなく、タスクが名指す exe で動きます。
4. 打つ端末は画面の真ん中から外しておきます。一瞬だけ前に出るコンソールの窓が見やすくなります。

いま答えている中継の `start` の記録:
`Get-Content "$env:LOCALAPPDATA\op-reason\ssh-log.jsonl" | Select-String '"what":"start"' | Select-Object -Last 1`。

| # | すること | 期待すること |
| --- | --- | --- |
| N1 | `& $exe --settings set relay.autostart on` | `relay.autostart: off → on`、そして `& $exe --ssh-agent --status` が 0 で終わる。コンソールの窓 (Windows Terminal の窓やタブも) が一瞬も出ない。`start` の記録に `"detail":"console: none"` |
| N2 | ログオフしてログオンし直す | ログオンの後、中継が答える (`& $exe --ssh-agent --status` が 0)。コンソールの窓は出なかった。その `start` の記録は `console: none` |
| N3 | `& $exe --settings`。窓でモードを `show-only` にし、**適用**、OK | 中継の行が `show-only`。起こし直しでコンソールの窓が出ない。モードを戻してもう一度 **適用**: やはり出ない |
| N4 | 通知領域の中継のアイコン: **中継を起こし直す** | 15 秒以内に別の process ID の中継が答える。コンソールの窓が出ない |
| N5 | 1Password をロックして解除する。`$env:OPREASON_SSH_REASON = 'check N5'; ssh -T git@github.com`。**この呼び出しを許可**。1Password で承認 | G4 と同じ: 押したときに op-reason の窓が前面に出て、1Password の画面がすぐ出る (名前は op-reason)。`Hi <user>!` |
| N6 | Explorer で `op-reason.exe` をダブルクリック。続けてタスクバーにピン留めし (右クリック → **その他のオプションを確認** → **タスクバーにピン留めする**)、窓を閉じてからタスクバーから起こす | どちらでも設定の窓が開くか前に出る。コンソールの窓は、タスクバーにも出ない。残さないなら、あとでピン留めを外す |
| N7 | `& $exe --settings set start-menu on`。Windows キーを押して `op-reason` と打ち、開く | 設定の窓。タスクバーにコンソールの窓が出ない。残さないなら、あとで `set start-menu off` |
| N8 | WSL で、`OPREASON_EXE` に `$exe` の WSL のパスを入れて `op-reason --reason 'check N8' vault list </dev/null 2>&1 \| cat` (呼び出しに端末が無い: WSL はこのとき exe を `CREATE_NO_WINDOW` で起こす) | いつもどおり op-reason の窓と 1Password の確認。vault の一覧が `cat` を通って出る。`op.exe` のコンソールの窓が出ない |
| N9 | PowerShell で `& $exe --reason 'check N9' vault list; $LASTEXITCODE` と `& $exe --ssh-agent --status; $LASTEXITCODE` | 今までどおり vault の一覧と 0、呼び出しが終わるとプロンプトが戻る。中継の行と 0 |
| N10 | コンソールの無い呼び出し: `Start-Process $exe -ArgumentList '--reason "check N10" vault list'` (1 つの文字列にし、理由を二重引用符で囲む: `Start-Process` は配列の要素を引用符なしで空白でつなぐ)。1Password で承認。そのあと `& $exe --log --since 5m` | いつもどおり op-reason の窓と 1Password の確認。コンソールの窓が一つも出ない (`Start-Process` は Windows 11 24H2 より前は自分のコンソールを付けて起こし、今は付けない。そのとき `op.exe` は見えないコンソールを持つ)。記録にその呼び出しの終わり (終了コード 0) |
| N11 | `& $exe --settings set relay.autostart off`。続けて同じ PowerShell で `& $exe --ssh-agent` を起こし、最初の行が出たら Ctrl+C。用意 3 のほかのタスクを有効に戻す | タスクが消える。手で起こした中継は行を出して端末に留まり、Ctrl+C で止まる。その `start` の記録は `console: shared`。ほかのタスクの中継がまた答える |

## 窓の見た目を確かめること (V1〜V6)

Windows のアプリのモードが暗いとき・明るいときの、`op-reason.exe` の窓と中継の窓
([2026-11-26-modern-window-design.md](superpowers/specs/2026-11-26-modern-window-design.md))。CI は両方のモードのスクショを
撮りますが、1Password の画面を横に出すことも、人のデスクトップで見ることもできません。約 15 分で、承認の画面は 5 回ほど
出ます。pull request の Windows の zip (用意のとおり。下では `$exe`) と、SSH の中継の確認の用意の中継 (`$exe` で端末から
起こしたもの) を使います。暗いモードから始めます: 設定 → 個人用設定 → 色 → モードを選ぶ → ダーク。

| # | すること | 期待すること |
| --- | --- | --- |
| V1 | `op --reason "check V1: dark" vault list >/dev/null`。承認する前に op-reason の窓を見て、1Password の画面がどこに出たかを控える | 右上に暗い窓が出る: 題名の無いキャプション (閉じるボタンだけ) が窓と同じ色で、角が丸い。錠前の青いタイル・題名・「op-reason · 1Password に承認を求めています」・時刻と日付。「理由」と、大きな字の理由。呼び手・repository と branch・ディレクトリのチップ。枠の中のコマンドで、`op vault` が青い。画面の絵のある帯。盾の付いた赤い行。右下の丸い言語の切り替え。1Password の画面が、帯のとおり画面の中央に出たかを控える |
| V2 | `op --reason "check V2: light while open" vault list >/dev/null`。窓が出ている間に、設定でモードをライトにする。承認する | 窓はすぐ明るくなり (色とキャプション)、場所と配置はそのまま。1Password の画面がフォーカスを持ったまま |
| V3 | 明るいモードで、PowerShell で `$env:OPREASON_SSH_REASON = 'check V3: light'; ssh -T git@github.com`。窓を見てから **この呼び出しを許可** を押し、承認する | 明るい窓: 鍵のタイル。理由。人・repository・ディレクトリのチップ。枠の中の起点のコマンド `ssh -T git@github.com`。小さな字の「git としてログイン」・接続先・鍵。答えを待つ残り秒の帯と「許可は、この ssh が終わるまで効きます (長くても 60 分)」。帯。ボタンは **この呼び出しを許可** (青)・**このターミナルを 60 分許可** (枠)・右端の **拒否** (赤い枠)。押した後は「SSH の鍵: 1Password が確認しています」で、帯がトレイを指す |
| V4 | `ssh` が `op-reason-ssh` を通る WSL の Claude Code に、SSH の remote のある repository で `git ls-remote origin` を打たせる。**拒否** | 枠には Claude Code が包む shell ではなく `git ls-remote origin` が出て、その下に起点の行が小さく出る (`bash (pid …) · bash -c "source …`)。「許可は、この git ls-remote が終わるまで効きます (長くても 60 分)」 |
| V5 | 画面の拡大率を 150% にし (設定 → システム → ディスプレイ)、`op --reason "check V5: この拡大率で窓の 2〜3 行を占めるように長く長く書いた理由で、配置を見るためのもの" vault list >/dev/null` を承認する。拡大率を戻す | 150% でも窓の字がにじまない。理由が折り返され、切り替えや帯に重ならず、窓は画面の中に収まる |
| V6 | S20 の 4 つのコマンドを、暗いモードと明るいモードで 1 回ずつ | カードは窓と同じ色: 暗い (明るい) 地に細い枠、緑の「通過」とチェック、等幅の字のコマンドと接続先。場所・大きさ・時間は前のまま |

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
