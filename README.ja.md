# op-reason

English: [README.md](README.md)

> **非公式の道具です。** op-reason は個人の project で、1Password (AgileBits Inc.) が作ったもの・認めたもの・
> 支えるものではありません。1Password は AgileBits Inc. の商標です。

op-reason は、WSL と Windows で使う [1Password CLI](https://developer.1password.com/docs/cli/) (`op`) の wrapper です。呼ぶたびに

1. 理由が要ります: `op --reason "<なぜ>" <op の引数…>`
2. 1Password のデスクトップアプリの承認を、その 1 回の呼び出しに閉じ込めます
3. 1Password の承認の画面と一緒に、誰が・どこで・なぜ・何を打つかを出す窓を出します
4. 記録に残します (`op-reason-log`)

![アニメーション: WSL の端末で Claude Code が op --reason "staging の VM を作るのに Proxmox の API の token を読む" read "op://Private/proxmox/token" を打つ。画面の右上に op-reason の窓が開いて時刻・呼び手 (Claude Code と process id)・場所・理由・コマンドを出し、画面の真ん中に 1Password の承認の画面 (素の箱で描いたもの) が出る。許可を押すと op が値を出し (図では伏せた)、op-reason が sign out して窓が消え、op-reason-log にその呼び出しが 1 行で出る (許可・Claude・理由・コマンド)。](docs/images/readme-1-call.ja.svg)

AI のコーディングエージェント (Claude Code など) に `op` を打たせるために作りました。承認する前に何のための依頼かが分かり、
1 回の承認が後の依頼に届かず、後からすべての呼び出しを見返せます。

## なぜ wrapper が要るのか

- **1 回の承認が後の呼び出しに漏れる。** 1Password のデスクトップアプリは、CLI の承認を `op.exe` を起こしたプロセスに
  付けます。WSL からはどの `op.exe` も、すべての端末が共有する同じ `wslhost` の子になるので、1 回の承認が後の呼び出しにも
  ほかの端末にも届きます。op-reason は呼び出しごとに Windows で新しい `op-reason.exe` を起こし、それが `op.exe` を起こして、
  終わったら sign out します。Windows から呼ぶときは、`op-reason.exe` 自身が呼び出しごとの新しいプロセスです。
- **画面は誰が頼んでいるかを言わない。** 1Password の承認の画面は、どのプロセスが頼んでも端末の名前 (「WindowsTerminal」、
  WSL からは「wslhost」) しか出しません。op-reason の窓は、呼び手 (人か、process id つきの Claude Code)・repo・branch・
  ディレクトリ・理由・コマンドを出します。
- **エージェントが何をしたかが残らない。** op-reason はすべての呼び出し・断り・見つけたことを JSON Lines で記録します。

![アニメーション。2 つの画面を並べる。左は WSL から op だけを使うとき: 端末 1 が op を呼び、画面の真ん中に出る 1Password の承認の画面で 1 回許可すると、端末 2 が op を呼んだときは画面なしで値が出る。右は op-reason: 呼ぶたびに承認の画面と一緒に、右上の op-reason の窓が呼び手と理由を出すので、端末 2 もまた承認を求められ、誰が・なぜが分かる。](docs/images/readme-2-approval-leak.ja.svg)

## 仕組み

![op-reason の部品](docs/architecture/plantuml/out/01_overview.svg)

WSL からの呼び出しでは、WSL の実行ファイルが理由を確かめ、呼び手と場所を集め、Windows で新しい `op-reason.exe --run` を
起こして呼び出しを渡します。`op-reason.exe` は WSL と Windows で共通の lock を取り (承認の画面は一度に 1 つ)、画面の右上に
窓を出し、あなたの stdin・stdout・stderr のまま `op.exe` を起こし、`op` が終わると `op.exe whoami` を打ち、session があれば
`op.exe signout --all` を打ちます。Windows から呼ぶときは、`op-reason.exe` がこれを全部自分のプロセスで行います
(lock・窓の文・記録は同じです)。

`op-reason.exe` が動かないところ (署名が無いので、Smart App Control に止められることがあります) では、PowerShell の
launcher `launcher.ps1` が新しい `powershell.exe` で同じ仕事をします。WSL からの呼び出しはひとりでにそちらへ回り、Windows
では PowerShell の入口がそれを使います。詳しくは [docs/architecture/README.md](docs/architecture/README.md) (英語)。

## 要るもの

- WSL の側: Windows の interop が使える WSL 2 (Windows の `PATH` が WSL の `PATH` に入っていて、`op.exe`・`op-reason.exe`・
  `wslpath` が見つかること)
- Windows に、**設定 → 開発者 → 1Password CLI と連携** を入れた 1Password のデスクトップアプリ、1Password CLI
  (`op.exe`。たとえば `winget install AgileBits.1Password.CLI`)、op-reason の Windows の zip (WSL からの呼び出しも、その
  `op-reason.exe` で窓を出します)
- Windows PowerShell 5.1 (Windows に入っています)。回避経路 (`launcher.ps1` と PowerShell の入口) だけが使います

## 入れ方

### Release から

Release は v0.1.0 から始まります。各 Release に、x86_64 と aarch64 の Linux の静的な実行ファイルと Windows の zip が、
それぞれ SHA-256 のファイルつきであります。**WSL では両方を入れてください**: 下の Linux の実行ファイルと、
[Windows で](#windows-で) に書いた Windows の zip です (WSL からの呼び出しは、その `op-reason.exe` で窓を出します。両方とも
同じ版にしてください)。

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

WSL の実行ファイルは、`op-reason.exe` を `PATH` (WSL は Windows の `PATH` を足します) か `OPREASON_EXE` の指す所
(`/mnt/c/Users/<あなた>/AppData/Local/Programs/op-reason/op-reason.exe` のような WSL のパス) から探し、`launcher.ps1` は
その隣から探します。

### Nix で

```sh
nix profile install github:pollenjp/op-reason      # op-reason・op-reason-log
nix profile install github:pollenjp/op-reason#op   # 同じものに `op` を足したもの
```

flake の input にもできます: `packages.<system>.default` と `packages.<system>.op`。

### Windows で

各 Release に `op-reason-windows.zip` (と `.sha256`) があります:

- `op-reason.exe`: Windows の入口です。理由・lock・窓・`op.exe`・sign out・記録まで、1 回の呼び出しを 1 つの program で
  こなします。WSL からの呼び出しの窓を出すのもこれです。
- `launcher.ps1`: WSL からの呼び出しの回避経路です。
- `powershell-entry\`: PowerShell の入口です。Smart App Control に `op-reason.exe` を止められるマシン向けです (下)。
- `SKILL.md`・README・ライセンス。

展開して 1 つのフォルダーに置き、そのフォルダーを `PATH` に足します。PowerShell で:

```powershell
$dir = "$env:LOCALAPPDATA\Programs\op-reason"    # どのフォルダーでもよい (下で PATH に足す)
$base = 'https://github.com/pollenjp/op-reason/releases/latest/download'
$zip = "$env:TEMP\op-reason-windows.zip"
Invoke-WebRequest "$base/op-reason-windows.zip" -OutFile $zip -UseBasicParsing
Invoke-WebRequest "$base/op-reason-windows.zip.sha256" -OutFile "$zip.sha256" -UseBasicParsing
$want = (Get-Content "$zip.sha256").Split(' ')[0]
if ((Get-FileHash $zip -Algorithm SHA256).Hash -ne $want) { throw 'SHA-256 が合いません' }
Expand-Archive $zip -DestinationPath $env:TEMP -Force
New-Item -ItemType Directory -Force $dir | Out-Null
Copy-Item "$env:TEMP\op-reason-windows\*" $dir -Recurse -Force
& "$dir\op-reason.exe" --version
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dir) {
  [Environment]::SetEnvironmentVariable('Path', "$userPath;$dir", 'User')    # これから開く端末に効く
}
```

新しい `PATH` は、これから開く端末に効きます。WSL に効くのは `wsl --shutdown` の後です (か、`OPREASON_EXE` を入れます)。

- **`op-reason.exe` には署名がありません。** 起こす前に Windows が警告を出すことがあり (SmartScreen)、Smart App Control が
  オンだと起こすこと自体を断ることがあります (新しい版でもまた断られえます)。そのとき WSL からの呼び出しは、ひとりでに
  `launcher.ps1` へ回ります (stderr に 1 行そう出ます)。Windows では下の PowerShell の入口を使ってください。
- PowerShell・cmd.exe・ほかの program のどれからも同じように呼べ、引数はそのまま `op` に届きます (`--` も引用符なしで
  届きます)。
- **PowerShell での出力**: PowerShell は program の出力を `[Console]::OutputEncoding` で読みます。`op` の出力に ASCII 以外が
  あるときは、`$x = op-reason …` の前に `[Console]::OutputEncoding = [Text.Encoding]::UTF8` を打ってください。binary の
  出力は `op` の `--out-file` で受けてください。
- **記録**は `%LOCALAPPDATA%\op-reason\log.jsonl` です (WSL と同じ形)。`op-reason --log` で見られます。
- WSL と Windows の呼び出しは一度に 1 つずつ走ります (`op-reason.exe` と launcher が、呼び出しの間 mutex `Local\op-reason`
  を持ちます)。
- **WSL からの呼び出し**: `OPREASON_RUNNER=launcher` で `op-reason.exe` を飛ばします (Smart App Control に止められるなら、
  毎回の試しとその通知が無くなります)。`OPREASON_RUNNER=exe` なら回避しません。

#### Smart App Control に op-reason.exe を止められたら: PowerShell の入口

zip の `powershell-entry` フォルダーに、`op-reason.ps1` (入口)・`op-reason.cmd` (cmd.exe やほかの program 向けの shim)・
`launcher.ps1` と `SKILL.md` の写しがあります。Windows に入っている Windows PowerShell で動く script で、Smart App Control が
オンのマシンでも動きました。そのかわり遅く、窓が出るのは呼んでから 1〜1.5 秒ほど後です (`op-reason.exe` は数十ミリ秒。
GitHub の Windows の runner で測った値)。`<フォルダー>\powershell-entry` を `PATH` の `<フォルダー>` **より前に**足して
ください。`op-reason` で PowerShell の入口が走り、WSL はそのまま `<フォルダー>` の `op-reason.exe` と `launcher.ps1` を
見つけます (WSL からの呼び出しは何も変えなくてよい)。入口だけ別のフォルダーなのは、PowerShell が同じフォルダーの
`op-reason.exe` より `op-reason.ps1` を先に選ぶためです。

- **PowerShell から**は、`op-reason` で `op-reason.ps1` が走ります。script を許す必要があります: 既定の実行の方針
  (Restricted) は script を断り、インターネットから来たファイルには Mark of the Web が付きます。
  `Set-ExecutionPolicy -Scope CurrentUser RemoteSigned` と `Unblock-File <フォルダー>\powershell-entry\*.ps1` を打つか、代わりに
  `op-reason.cmd` を呼んでください。`-ExecutionPolicy Bypass` で動いている shell (Claude Code の tool の shell など) は、
  `op-reason.ps1` をそのまま呼べます。
- **cmd.exe やほかの program から**は `op-reason.cmd` を呼びます。cmd.exe は引数を読み直すので、空白の無い引数に `&`・`|`・
  `<`・`>`・`^`・`%` があると崩れることがあります (「BatBadBut」と呼ばれる問題)。そうした引数は引用符で囲むか、PowerShell
  から `op-reason.ps1` を呼んでください。
- **PowerShell の `--`**: 引用符で囲まない `--` は PowerShell が取ります。`op run` の子のコマンドの前には `'--'` と書いて
  ください: `op-reason --reason "…" run '--' my-command`。
- **PowerShell での出力**: `$x = op-reason …` は `op` の出力を文字 (UTF-8) として受け取ります。binary の出力は `op` の
  `--out-file` か、cmd.exe でリダイレクトした `op-reason.cmd` で受けてください。
- **記録**は同じファイルです。PowerShell の入口には記録を見るコマンドがありません:
  `Get-Content $env:LOCALAPPDATA\op-reason\log.jsonl | ConvertFrom-Json | Format-Table time, event, reason`

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

![アニメーション: 理由 (--reason) の無い op read "op://Private/proxmox/token" は、窓も承認の画面も出る前にすぐ断られる。短い使い方と「op-reason: 理由 (--reason) がありません。人が打つときも要ります。」が出て、echo $? で終了コード 2 が出る。](docs/images/readme-3-no-reason.ja.svg)

例は symlink の `op` で書いています。`op-reason` の名前で呼んでも同じ動きで、`--help` と `--version` を単独で渡したとき
だけ wrapper 自身のものになります。Windows では `op-reason` と打ちます。

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

![アニメーション: op-reason-log が 1 呼び出し 1 行で、時刻・結果 (許可・断った (理由なし)・拒否)・秒・呼び手 (Claude か人)・理由・コマンドを出す。呼び手・理由・結果の列が順に光る。](docs/images/readme-4-log.ja.svg)

記録は WSL では `${XDG_STATE_HOME:-~/.local/state}/op-reason/log.jsonl`、Windows では `%LOCALAPPDATA%\op-reason\log.jsonl`
です (1 MiB で `log.1.jsonl` へ回します。Windows の `op-reason --log` も同じ option を取ります)。`op` の出力と
token は書かず、`--session`・`--secret-key` と `name=value` の引数の値は伏せます。記録の形は
[docs/architecture/README.md](docs/architecture/README.md) にあります。

### AI エージェント

op-reason を初めて使うエージェントは、最初に `op-reason --skill` を打ってください。理由をつけた `op` の呼び方と、
拒否・時間切れのときにすること (打ち直さず、ユーザーに聞く) が書いてあります。

### 表示の言語

既定は英語です。窓の右下の **日本語** (か **English**) を押すと切り替わります。窓はその場で変わり、1Password の承認の
画面からフォーカスを奪いません。次の呼び出しからは、窓・通知・文・`op-reason-log` がその言語になります (Windows でも
WSL でも)。選んだ言語は `%LOCALAPPDATA%\op-reason\settings.json` に残ります。消すと `OPREASON_LANG` に戻ります (WSL
では控えの `~/.local/state/op-reason/lang.json` も消すか、次の呼び出しが消すのを待ってください)。

### 終了コード

| コード | 意味 |
| --- | --- |
| `op` の終了コード | 呼び出しが走った |
| 2 | 理由が無い・理由が空・引数が Windows に渡せないほど長い・wrapper の option の誤り |
| 75 | ほかの呼び出し (WSL か Windows) の承認待ちが終わらない (lock を待つ上限を超えた) |
| 127 | `op.exe`・`op-reason.exe` (とその回避経路)・`launcher.ps1`・`powershell.exe`・`wslpath` が見つからない・起こせない、または `wslpath -w` がパスを変換できない |
| 1 | `op-reason.exe` か launcher が `op.exe` を起こせなかった |
| 129・130・143 | SIGHUP・SIGINT・SIGTERM で止められた (WSL)。Windows では Ctrl+C が 130、`op-reason.exe` のコンソールが閉じられたら 129 |

### 環境変数

| 変数 | 意味 |
| --- | --- |
| `OPREASON_LANG` | `en` か `ja`: 窓で言語を選んでいない間の、文・窓・記録の表の言語 ([表示の言語](#表示の言語))。既定は英語 (`LANG` と Windows の表示の言語は見ません) |
| `OPREASON_LOCK_WAIT` | ほかの呼び出しの承認待ちを待つ秒 (既定 90。Claude Code の Bash の既定の時間切れ 120 秒より短い) |
| `OPREASON_OP_EXE` | `op.exe` のパス (既定は `PATH` の `op.exe`) |
| `OPREASON_EXE` | WSL: `op-reason.exe` のパス (既定は `PATH` の `op-reason.exe`)。`launcher.ps1` はそのフォルダーから探します |
| `OPREASON_RUNNER` | WSL: `auto` (既定。`op-reason.exe` を起こし、それが呼び出しを始めなかったときだけ `launcher.ps1`)・`exe` (launcher を使わない)・`launcher` (exe を使わない) |
| `OPREASON_POWERSHELL` | WSL: launcher を起こす `powershell.exe` のコマンド (既定 `powershell.exe`) |
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
- **呼び出しを止める道具。** 承認の画面が出ている間に道具が `op-reason` を止めると (コマンドの時間切れなど)、窓が消えて
  画面だけが残ることがあります。拒否してください。許可してしまったら 1Password をロックしてください (承認は、終わった
  プロセスに付いたまま 1Password の 10 分の期限まで残ります)。`op-reason.exe` そのものが止められたときは、`op.exe` と
  その画面も一緒に終わります (Job Object)。WSL から止めたときにそれが `op-reason.exe` へどう届くかは、まだ手で確かめていま
  せん ([docs/manual-verification.ja.md](docs/manual-verification.ja.md))。
- **共有の承認。** op-reason は各呼び出しの前と後に、WSL から直に `op.exe whoami` を打ちます。通れば、どの端末からも使える
  承認がある (窓の出ていない画面を許可したか、道具が `op.exe` を直に打った) ので、sign out して記録し、通知を出します。
- **Windows の Ctrl+C**: `op-reason.exe` は `op` を止め、sign out し、`中断` と記録して 130 で終わります。PowerShell の入口
  では、Ctrl+C で止めた呼び出しは、PowerShell が入口の後始末を走らせれば `中断` (終了コード 130) として記録します。
  PowerShell のプロセス自身の終了コードは PowerShell が決めます。
- **Windows のパイプ**: Windows では op-reason どうしのパイプ (`op-reason … | op-reason …`) を見分けません。2 つ目の呼び出しは
  `OPREASON_LOCK_WAIT` が尽きるまで lock を待つことがあります。1 つ目の出力は変数に入れてください。
- WSL の見張りは Windows の lock の外で `op.exe signout --all` を打つので、共有の承認を見つけたときに、その時動いている
  Windows の呼び出しの session を閉じることがあります (まれ)。
- まだ確かめていないこと: `--all` の無い `op signout` が、1 つのプロセスの session だけを閉じるか。

## 開発

```sh
nix develop path:./nix                       # Rust・pwsh・fish・jq・shellcheck・shfmt・python3
cargo test --workspace                       # 単体テストと呼び出しの表
cargo build && bash tests/run-all.sh         # 偽の op-reason.exe・powershell.exe・op.exe での黒箱のテスト
python3 docs/images/make_readme_svgs.py      # README の動く図 (docs/images/*.svg)
# op-reason.exe は別の Cargo workspace: crates/op-reason-win で、Windows なら `cargo build -p fake-op` のあと
# `cargo test --workspace` (Linux では Windows に依らない部分だけ)
```

- README の動く図は `docs/images/make_readme_svgs.py` (標準ライブラリだけの Python) で作ります。文言や配置を
  直すときは SVG ではなく script を直し、devShell の中で流して作り直してください。図の文は op-reason の文 (窓・断り・
  記録の表) の写しです。CI が流す `--check` は、SVG が script の出力と違うときと、写した文が Rust のソースから
  消えたときに落ちます。
- 本物の 1Password で確かめる手順: [docs/manual-verification.ja.md](docs/manual-verification.ja.md)
- 設計: [docs/superpowers/specs/2026-11-15-op-reason-design.md](docs/superpowers/specs/2026-11-15-op-reason-design.md) (英語)。
  `op-reason.exe` は [docs/superpowers/specs/2026-11-15-windows-exe-design.md](docs/superpowers/specs/2026-11-15-windows-exe-design.md) (英語)、
  WSL からの呼び出しを exe に通す形は [docs/superpowers/specs/2026-11-15-wsl-through-exe-design.md](docs/superpowers/specs/2026-11-15-wsl-through-exe-design.md) (英語)、
  窓の表示の言語は [docs/superpowers/specs/2026-11-15-language-toggle-design.md](docs/superpowers/specs/2026-11-15-language-toggle-design.md) (英語)
- 版ごとに変わったこと: [CHANGELOG.md](CHANGELOG.md) (英語)
- どこに何があるか: [docs/README.md](docs/README.md)

## ライセンス

[Apache License, Version 2.0](LICENSE-APACHE) と [MIT license](LICENSE-MIT) のどちらかを選んで使えます。
