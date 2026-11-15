# 本物の 1Password で op-reason を確かめる

English: [manual-verification.md](manual-verification.md)

CI は偽の `powershell.exe` と `op.exe` でしか op-reason を流せません (1Password のデスクトップアプリは CI で動きません)。
ここの確認には、人と、1Password のアプリと 1Password CLI の入った Windows と、WSL が要ります。20 分ほどかかり、承認の
画面が 10 回ほど出ます。

## 確かめる間の決まり

- 出力にシークレットの無いコマンドだけを使います: `vault list`・`whoami`・`item template get`・`item create --dry-run`。
  手順に無い限り、出力は `/dev/null` に捨てます。`op read`・`item get`・`--reveal` は打ちません。
- 画面には手順どおりに応えます。手順が見込んでいない画面が出たら拒否し、書き留めます。
- 終わったら 1Password を 1 回ロックします。

## 用意

```sh
cargo build --release
export PATH=$PWD/target/release:$PATH
ln -sf op-reason target/release/op && ln -sf op-reason target/release/op-reason-log
command -v op            # …/target/release/op
op.exe --version         # 本物の 1Password CLI が答える
export OPREASON_LANG=ja  # 英語なら en
```

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
| 11 | `timeout 5 op --reason "確認 11: 止める" vault list`、応えない | 5 秒で窓が消え、画面だけが残る。拒否する。`op-reason-log` に `中断` |
| 12 | `op --version`・`op --help`・`op item get --help` | 理由なしで通り、窓も画面も出ない |
| 13 | `op item get x` (理由なし) | 終了コード 2、AI エージェント向けの案内つきの使い方。`op-reason-log` に `断った (理由なし)` |
| 14 (任意) | service account があれば、token を表示せずに `OP_SERVICE_ACCOUNT_TOKEN` に入れ、`op --reason "確認 14: service account" vault list >/dev/null` | 窓も画面も出ず、終了コード 0。`op-reason-log` に `成功 (SA)` |

## Windows で確かめること: op-reason.exe

Release (か、pull request の Windows exe の workflow の `op-reason-exe` の artifact) から `op-reason.exe` を落とし、README の
とおりに SHA-256 を確かめます。`PATH` にある PowerShell の入口が代わりに応えないよう、`op-reason.exe` と名前を全部書いて
呼びます。約 15 分かかり、画面は 10 回ほど出ます。

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

## Windows で確かめること: PowerShell の入口 (回避経路)

Release の zip (か、repo の `windows/op-reason.ps1`・`windows/op-reason.cmd`・`launcher/launcher.ps1` を `PATH` の 1 つの
フォルダーに並べたもの) を使います。

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
