# op-reason

English: [README.md](README.md)

> **非公式の道具です。** op-reason は個人の project で、1Password (AgileBits Inc.) が作ったもの・認めたもの・
> 支えるものではありません。1Password は AgileBits Inc. の商標です。

op-reason は、WSL と Windows で使う [1Password CLI](https://developer.1password.com/docs/cli/) (`op`) の wrapper です。呼ぶたびに

1. 理由が要ります: `op --reason "<なぜ>" <op の引数…>`
2. 1Password のデスクトップアプリの承認を、その 1 回の呼び出しに閉じ込めます
3. 1Password の承認の画面と一緒に、誰が・どこで・なぜ・何を打つかを出す窓を出します
4. 記録に残します (`op-reason-log`)

AI のコーディングエージェント (Claude Code など) に `op` を打たせるために作りました。承認する前に何のための依頼かが分かり、
1 回の承認が後の依頼に届かず、後からすべての呼び出しを見返せます。

## なぜ wrapper が要るのか

- **1 回の承認が後の呼び出しに漏れる。** 1Password のデスクトップアプリは、CLI の承認を `op.exe` を起こしたプロセスに
  付けます。WSL からはどの `op.exe` も、すべての端末が共有する同じ `wslhost` の子になるので、1 回の承認が後の呼び出しにも
  ほかの端末にも届きます。op-reason は呼び出しごとに新しい `powershell.exe` を起こしてそこから `op.exe` を起こし、終わったら
  sign out します。
- **画面は誰が頼んでいるかを言わない。** 1Password の承認の画面は、どのプロセスが頼んでも端末の名前 (「WindowsTerminal」、
  WSL からは「wslhost」) しか出しません。op-reason の窓は、呼び手 (人か、process id つきの Claude Code)・repo・branch・
  ディレクトリ・理由・コマンドを出します。
- **エージェントが何をしたかが残らない。** op-reason はすべての呼び出し・断り・見つけたことを JSON Lines で記録します。

## 仕組み

![op-reason の部品](docs/architecture/plantuml/out/01_overview.svg)

入口 (WSL の実行ファイルか、Windows の `op-reason.ps1`) は、呼び出しごとに理由を確かめ、呼び手を集め、新しい
`powershell.exe` で launcher (PowerShell の script。WSL の実行ファイルは埋め込んでいる) を起こします。launcher は WSL と
Windows で共通の lock を取り (承認の画面は一度に 1 つ)、画面の右上に窓を出し、あなたの stdin・stdout・stderr のまま
`op.exe` を起こし、`op` が終わると `op.exe whoami` を打ち、session があれば `op.exe signout --all` を打ちます。詳しくは
[docs/architecture/README.md](docs/architecture/README.md) (英語)。

## 要るもの

- WSL の側: Windows の interop が使える WSL 2 (Windows の `PATH` が WSL の `PATH` に入っていて、`op.exe`・`powershell.exe`・
  `wslpath` が見つかること)
- Windows に、**設定 → 開発者 → 1Password CLI と連携** を入れた 1Password のデスクトップアプリと、1Password CLI
  (`op.exe`。たとえば `winget install AgileBits.1Password.CLI`)
- Windows PowerShell 5.1 (Windows に入っています)

## 入れ方

### Release から

Release は v0.1.0 から始まります。各 Release に、x86_64 と aarch64 の Linux の静的な実行ファイルと SHA-256 のファイルが
あります。

```sh
arch=$(uname -m)   # x86_64 か aarch64
base=https://github.com/pollenjp/op-reason/releases/latest/download
curl -fLO "${base}/op-reason-${arch}-linux"
curl -fLO "${base}/op-reason-${arch}-linux.sha256"
sha256sum -c "op-reason-${arch}-linux.sha256"
install -m 755 "op-reason-${arch}-linux" ~/.local/bin/op-reason
ln -sf op-reason ~/.local/bin/op-reason-log
ln -sf op-reason ~/.local/bin/op     # 任意: `op` を wrapper にする (PATH で ~/.local/bin を先に置く)
```

### Nix で

```sh
nix profile install github:pollenjp/op-reason      # op-reason・op-reason-log
nix profile install github:pollenjp/op-reason#op   # 同じものに `op` を足したもの
```

flake の input にもできます: `packages.<system>.default` と `packages.<system>.op`。

### Windows で

各 Release に `op-reason-windows.zip` (と `.sha256`) があります。中身は `op-reason.ps1` (入口)・`op-reason.cmd`
(cmd.exe やほかの program 向けの shim)・`launcher.ps1`・`SKILL.md` です。展開してファイルをまとめて置き、そのフォルダーを
`PATH` に足します。

- **PowerShell から**は、`op-reason` で `op-reason.ps1` が走ります。script を許す必要があります: 既定の実行の方針
  (Restricted) は script を断り、インターネットから来たファイルには Mark of the Web が付きます。
  `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` と `Unblock-File <フォルダー>\*.ps1` を打つか、代わりに
  `op-reason.cmd` を呼んでください。`-ExecutionPolicy Bypass` で動いている shell (Claude Code の tool の shell など) は、
  `op-reason.ps1` をそのまま呼べます。
- **cmd.exe やほかの program から**は `op-reason.cmd` を呼びます。cmd.exe は引数を読み直すので、空白の無い引数に `&`・`|`・
  `<`・`>`・`^`・`%` があると崩れることがあります (「BatBadBut」と呼ばれる問題)。そうした引数は引用符で囲むか、PowerShell
  から `op-reason.ps1` を呼んでください。
- **PowerShell の `--`**: 引用符で囲まない `--` は PowerShell が取ります。`op run` の子のコマンドの前には `'--'` と書いて
  ください: `op-reason --reason "…" run '--' my-command`。
- **PowerShell での出力**: `$x = op-reason …` は `op` の出力を文字 (UTF-8) として受け取ります。binary の出力は `op` の
  `--out-file` か、cmd.exe でリダイレクトした `op-reason.cmd` で受けてください。
- **記録**は `%LOCALAPPDATA%\op-reason\log.jsonl` です (WSL と同じ形)。Windows にはまだ記録を見るコマンドがありません:
  `Get-Content $env:LOCALAPPDATA\op-reason\log.jsonl | ConvertFrom-Json | Format-Table time, event, reason`
- WSL と Windows の呼び出しは一度に 1 つずつ走ります (launcher が呼び出しの間、mutex `Local\op-reason` を持ちます)。

### fish の補完

```sh
op-reason --completion fish >~/.config/fish/completions/op.fish
op-reason --completion fish op-reason >~/.config/fish/completions/op-reason.fish
```

この補完は `op` を呼びません (fish に同梱の `op` の補完は `op completion fish` を打ち、wrapper は理由が無いので断ります)。
Nix の package は補完も入れます。

## 使い方

```sh
op --reason "staging の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token"
```

- 理由は、何のために何を読む・書くかを 1 文で書きます。窓と記録に出ます。
- `--reason "<理由>"` でも `--reason=<理由>` でもよく、`--` より前ならどこに置いてもかまいません (`--` より後ろは、
  `op run` の子のコマンドのように `op` の引数です)。
- `op --version`・`op --help`・`op help <コマンド>`・`op <コマンド> --help` は理由なしで通ります (画面も窓も出ません)。
- 引数なしの `op` は wrapper の使い方を出します。
- **op-reason の窓が出ていない 1Password CLI の承認の画面は、拒否してください。**

例は symlink の `op` で書いています。`op-reason` の名前で呼んでも同じ動きで、`--help` と `--version` を単独で渡したとき
だけ wrapper 自身のものになります。

### wrapper の option (最初の引数のときだけ)

| コマンド | すること |
| --- | --- |
| `op-reason --help`・`op-reason -h` | wrapper の使い方 |
| `op-reason --version`・`op-reason -V` | op-reason の版 |
| `op-reason --skill` | AI エージェント向けの手引き (SKILL.md)。断るときの文からも案内します |
| `op-reason --completion fish [<コマンド>]` | `op` (か `<コマンド>`) の fish の補完 |
| `op-reason-log`・`op-reason --log` | 記録を見る (下) |

### 記録

```sh
op-reason-log              # 直近 1 時間に始まった呼び出し
op-reason-log --since 1d   # 30m・2h・1d…
op-reason-log --all        # 残っている記録を全部
op-reason-log --json       # 1 呼び出し 1 行の JSON (理由とコマンドを切らない)
```

記録は `${XDG_STATE_HOME:-~/.local/state}/op-reason/log.jsonl` です (1 MiB で `log.1.jsonl` へ回します)。`op` の出力と
token は書かず、`--session`・`--secret-key` と `name=value` の引数の値は伏せます。記録の形は
[docs/architecture/README.md](docs/architecture/README.md) にあります。

### AI エージェント

op-reason を初めて使うエージェントは、最初に `op-reason --skill` を打ってください。理由をつけた `op` の呼び方と、
拒否・時間切れのときにすること (打ち直さず、ユーザーに聞く) が書いてあります。

### 終了コード

| コード | 意味 |
| --- | --- |
| `op` の終了コード | 呼び出しが走った |
| 2 | 理由が無い・理由が空・引数が Windows に渡せないほど長い・wrapper の option の誤り |
| 75 | ほかの呼び出し (WSL か Windows) の承認待ちが終わらない (lock を待つ上限を超えた) |
| 127 | `op.exe`・`powershell.exe`・`wslpath` が見つからない、または `powershell.exe` を起こせない |
| 1 | launcher が `op.exe` を起こせなかった |
| 129・130・143 | SIGHUP・SIGINT・SIGTERM で止められた (WSL)。Windows は Ctrl+C を 130 と記録 |

### 環境変数

| 変数 | 意味 |
| --- | --- |
| `OPREASON_LANG` | `en` か `ja`: 文・窓・記録の表の言語。無ければ `LC_ALL`・`LC_MESSAGES`・`LANG` (`ja…` なら日本語)、どれも無ければ英語 |
| `OPREASON_LOCK_WAIT` | ほかの呼び出しの承認待ちを待つ秒 (既定 90。Claude Code の Bash の既定の時間切れ 120 秒より短い) |
| `OPREASON_OP_EXE` | `op.exe` のパス (既定は `PATH` の `op.exe`) |
| `OPREASON_POWERSHELL` | `powershell.exe` のコマンド (既定 `powershell.exe`) |
| `OP_SERVICE_ACCOUNT_TOKEN`、`OP_CONNECT_HOST` と `OP_CONNECT_TOKEN` | service account か Connect: `op` はデスクトップアプリなしで走り (窓も画面も無し)、token を受け取ります。デスクトップの呼び出しには渡しません |
| `HERDR_ENV`・`HERDR_WORKSPACE_ID`・`HERDR_PANE_ID` | [herdr](https://herdr.dev/) の中では、窓に workspace と pane を出します (任意) |

接頭辞が `OP_` でなく `OPREASON_` なのは、1Password CLI 自身が `OP_…` の変数を多く読むためです。

## 知っておくこと

- **`signout --all` は CLI の session をすべて閉じます。** この呼び出しのものだけでなく、同じときにほかの端末のタブで手で
  打った `op` のものも閉じます。op-reason の呼び出しどうしは一度に 1 つなので、互いの session は閉じません (パイプで
  つないだ 2 つの呼び出しは、送り手が session に触らない形で扱います)。
- **一度に 1 つ。** ある呼び出しが承認を待つ間、次の呼び出しは `OPREASON_LOCK_WAIT` 秒まで待ち、超えたら 75 で終わります。
  `op … | op …` は動きます (受け手が、待つ前に送り手の出力を読み切ります)。
- **長く動く `op run`。** lock と窓は `op` が終わるまで残ります。server を `op run` で起こさず、値を解いたら `op` を終わらせて
  ください。
- **呼び出しを止める道具。** 承認の画面が出ている間に道具が `op-reason` を止めると (コマンドの時間切れなど)、窓は消えて
  画面だけが残ります。拒否してください。許可してしまったら 1Password をロックしてください (承認は、終わったプロセスに
  付いたまま 1Password の 10 分の期限まで残ります)。
- **共有の承認。** op-reason は各呼び出しの前と後に、WSL から直に `op.exe whoami` を打ちます。通れば、どの端末からも使える
  承認がある (窓の出ていない画面を許可したか、道具が `op.exe` を直に打った) ので、sign out して記録し、通知を出します。
- **Windows の Ctrl+C**: Ctrl+C で止めた呼び出しは、PowerShell が入口の後始末を走らせれば `中断` (終了コード 130) として
  記録します。PowerShell のプロセス自身の終了コードは PowerShell が決めます。
- **Windows のパイプ**: Windows では op-reason どうしのパイプ (`op-reason … | op-reason …`) を見分けません。2 つ目の呼び出しは
  `OPREASON_LOCK_WAIT` が尽きるまで lock を待つことがあります。1 つ目の出力は変数に入れてください。
- WSL の見張りは Windows の lock の外で `op.exe signout --all` を打つので、共有の承認を見つけたときに、その時動いている
  Windows の呼び出しの session を閉じることがあります (まれ)。
- まだ確かめていないこと: `--all` の無い `op signout` が、1 つのプロセスの session だけを閉じるか。

## 開発

```sh
nix develop path:./nix                       # Rust・pwsh・fish・jq・shellcheck・shfmt
cargo test --workspace                       # 単体テストと呼び出しの表
cargo build && bash tests/run-all.sh         # 偽の powershell.exe と op.exe での黒箱のテスト
```

- 本物の 1Password で確かめる手順: [docs/manual-verification.ja.md](docs/manual-verification.ja.md)
- 設計: [docs/superpowers/specs/2026-11-15-op-reason-design.md](docs/superpowers/specs/2026-11-15-op-reason-design.md) (英語)
- どこに何があるか: [docs/README.md](docs/README.md)

## ライセンス

[Apache License, Version 2.0](LICENSE-APACHE) と [MIT license](LICENSE-MIT) のどちらかを選んで使えます。
