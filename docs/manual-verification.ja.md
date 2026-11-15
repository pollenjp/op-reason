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
