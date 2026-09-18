# grapnel 仕様書

## 1. 設定ファイル

エントリファイルの例:

```toml
include = ["apps/*.toml"]          # エントリからの相対パス。glob 可

[settings]
initial_mode = "insert"
passthrough = ["games"]            # 前面にあるとフックを外すターゲット
suspend_hotkey = "C-M-S-p"         # C/M/S/W と通常キーのみ
gesture_threshold = 30             # ジェスチャーの 1 方向とみなす移動量 (px)

[modes.insert]
[modes.normal]
block_unmapped = true              # 定義外のキーを握りつぶす

[modifiers.Mu]
key = "Muhenkan"
tap = "Muhenkan"                   # 省略時は key と同じ。"" なら何も送らない
tap_timeout_ms = 300               # これより長く押したら tap を送らない。0 は無制限

[targets.editor]
app = "code.exe"
[targets.browser_input]
app = "glob:*chrome.exe"
uia_type = "Edit"
[targets.games]
any = ["steam_game", "emulator"]

[[rules]]
keys = "C-x t 0"
action = "close_tab"
targets = ["editor"]
on_mismatch = "discard"
timeout_ms = 1500

[[rules]]
keys = "Mu-j"
action = "down"
press = "hold"

[[actions.close_tab]]
when = ["editor"]
do = ["C-w"]

[[actions.down]]
do = ["Down"]

[[actions.search]]
do = [{ input = "検索", then = "open_search" }]

[[actions.open_search]]
do = [{ run = "cmd", args = ["/c", "start", "https://www.google.com/search?q={arg}"] }]
```

### 1.1 トップレベル

| キー | 型 | 説明 |
| --- | --- | --- |
| `include` | 文字列の配列 | そのファイルからの相対パス (glob 可)。include 先でも使える。同じファイルは 1 度だけ読む |
| `settings` | テーブル | エントリファイルにのみ書ける |
| `modes.<名前>` | テーブル | モードの定義 |
| `modifiers.<名前>` | テーブル | ユーザー定義修飾キー |
| `targets.<名前>` | テーブル | ターゲット |
| `rules` | テーブルの配列 | 入力ルール。読み込み順に連結する |
| `actions.<名前>` | テーブルの配列 | アクションの実装。読み込み順に連結する |

`modes` / `modifiers` / `targets` で同じ名前が複数のファイルにあればエラー。
読み込み順は、エントリ → include を書かれた順 (glob はパス名順) に深さ優先。

### 1.2 settings

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `initial_mode` | `"default"` | 起動時のモード |
| `passthrough` | `[]` | ターゲット名の配列 |
| `suspend_hotkey` | なし | 一時停止を切り替えるホットキー |
| `gesture_threshold` | `30` | px |

モード `default` は定義しなくても存在する。

### 1.3 modes.<名前>

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `block_unmapped` | `false` | そのモードで、どのルールにも合わないキーボードのキー (修飾キー以外) を握りつぶす。マウスは対象外 |
| `unmapped_to` | なし | そのモードで、どのルールにも合わない入力 (修飾キー以外。マウスボタン・ホイールを含む) が来たら、このモードに切り替える。入力自体はそのまま流す (`block_unmapped` なら握りつぶす)。`hold` の修飾キーが変わる場合は、先に修飾キーを離して (押して) からその入力を送る。Emacs のマークのように「他の操作をしたら抜ける」モードに使う |
| `hold` | なし | `"S"` や `"C-S"` のように C/M/S/W を `-` でつないだもの。そのモードにいる間、これらの修飾キーを押したままにする (Emacs のマークで Shift を押したままにして選択を広げる、など)。一時停止やパススルーで状態をリセットするときは、押したままにならないよう `initial_mode` に戻る |

### 1.4 modifiers.<名前>

名前は英字で始まり英数字からなる。`C` `M` `S` `W` は予約済み。

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `key` | 必須 | 修飾キーにする入力 (1 つのキー、マウスボタン、パッドボタン)。Ctrl/Alt/Shift/Win は不可 |
| `tap` | `key` と同じ | 単独でタップしたときに送るキー列 |
| `tap_timeout_ms` | `0` | 0 なら何秒押していても tap を送る |
| `as` | なし | `"C"` や `"C-S"` のように C/M/S/W を `-` でつないだもの。この修飾キーを押している間、ここに書いた修飾キーも押したままにする (例: macOS の Cmd を Ctrl として使う) |

### 1.5 targets.<名前>

書いた項目をすべて満たせば合う。何も書かなければ常に合う。

| キー | 照合する値 |
| --- | --- |
| `app` | 前面のプロセスの exe 名。値に `\` を含むときはフルパスと照合する (`re:` では、パス区切りを表す `\\` を含むときだけフルパス。`re:^emacs\.exe$` は exe 名と照合する) |
| `title` | 前面ウインドウのタイトル |
| `class` | 前面ウインドウのクラス名 |
| `control` | フォーカスのあるコントロールのクラス名 |
| `uia_id` / `uia_name` / `uia_type` | フォーカスのある UI Automation 要素の AutomationId / Name / ControlType (`Edit`, `Document` など) |
| `not` | ターゲット名。それに合わなければ合う |
| `any` | ターゲット名の配列。どれかに合えば合う |
| `all` | ターゲット名の配列。すべてに合えば合う |

文字列の照合:

- そのまま: 大文字小文字を区別しない完全一致
- `glob:パターン`: `*` と `?` が使える。大文字小文字を区別しない
- `re:正規表現`: 部分一致。大文字小文字の区別は `(?i)` で指定する

### 1.6 rules

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `keys` | 必須 | キー列 (2 章) |
| `action` | 必須 | アクション名 |
| `targets` | `[]` | どれかに合えば働く。空なら無条件 |
| `modes` | `[]` | 働くモード。空なら全モード |
| `press` | `"hold"` | `"hold"` / `"tap"` (3.3 節) |
| `fallback` | 元の入力 | 使える実装が無いときに送るキー列。`""` なら何も送らない |
| `on_mismatch` | `"replay"` | chord の途中で合わなかったとき: `"replay"` / `"discard"` / `"fallback"` |
| `timeout_ms` | `0` | chord の次の入力を待つ時間。0 は無制限 |
| `keep_mods` | `false` | 出力の修飾キーのうち入力で押していないもの (例: Alt) を、`keys` の最後の chord の修飾キーを離すまで押したままにする。ユーザー修飾キーならその `as` より優先し、実際の修飾キー (例: Alt-Tab → Ctrl-Tab の Alt) ならその間 OS には押していないものとして扱う。Alt-Tab や Ctrl-Tab の切り替え画面を開いたままにする用途。最後の chord に修飾キーが必要 |

### 1.7 actions.<名前>

| キー | 既定値 | 説明 |
| --- | --- | --- |
| `when` | `[]` | ターゲット名の配列。どれかに合えば使う。空なら常に使う |
| `do` | 必須 | 手順の配列 |

手順 (1 つの手順に書けるのは 1 種類だけ):

| 書き方 | 内容 |
| --- | --- |
| `"C-a"` または `{ keys = "C-a b" }` | キー列を順にタップする |
| `{ text = "文字列" }` | Unicode 文字列を入力する |
| `{ mouse_move = [dx, dy] }` | カーソルを相対移動する |
| `{ mouse_move_to = [x, y] }` | カーソルを画面座標へ移動する |
| `{ sleep = ミリ秒 }` | 待つ |
| `{ run = "プログラム", args = [...] }` | 起動する (終了は待たない) |
| `{ call = "アクション名", arg = "..." }` | 別のアクションを実行する。`arg` を省略すると現在の引数を渡す |
| `{ mode = "モード名" }` | モードを切り替える |
| `{ control = "suspend" }` | `"suspend"` (一時停止の切替) / `"reload"` / `"exit"` |
| `{ input = "プロンプト", then = "アクション名" }` | 入力欄を出し、確定したら入力値を引数にして `then` を実行する |

`text` / `args` / `arg` の中の `{arg}` は、そのアクションに渡された引数に置き換える (無ければ空文字列)。

## 2. キー列

```
キー列   = chord (" " chord)*
chord    = (修飾 "-")* キー
修飾     = "C" | "M" | "S" | "W" | ユーザー定義修飾キー名
```

- `C` = Ctrl, `M` = Alt, `S` = Shift, `W` = Win。左右は区別しない。
- 修飾名の後ろに `-` とそれ以降の文字列があるときだけ修飾として読む。`C--` は Ctrl + `-`。
- chord は修飾の組み合わせが完全に一致したときだけ合う (`C-x` は `C-S-x` に合わない)。
- キー名は大文字小文字を区別しない。

| 種類 | キー名 |
| --- | --- |
| 英数字 | `a`–`z`, `0`–`9` |
| 記号 (JIS/US の VK) | `-` `^` `\` `@` `[` `;` `:` `]` `,` `.` `/` `_` (VK_OEM_102) ほか `Oem1` 形式 |
| 制御 | `Enter` `Esc` `Tab` `Space` `Backspace` `Delete` `Insert` `Home` `End` `PageUp` `PageDown` `Up` `Down` `Left` `Right` `CapsLock` `PrintScreen` `ScrollLock` `Pause` `Apps` |
| ファンクション | `F1`–`F24` |
| テンキー | `Num0`–`Num9` `NumAdd` `NumSub` `NumMul` `NumDiv` `NumDot` `NumLock` |
| 日本語 | `Muhenkan` `Henkan` `Kana` `Zenkaku` (半角/全角), `Eisu` (英数) |
| 修飾キー単体 (出力専用) | `LCtrl` `RCtrl` `LAlt` `RAlt` `LShift` `RShift` `LWin` `RWin` |
| 直接指定 | `vk:0x41`, `sc:0x7B` (`sc:0xE01D` で拡張キー) |
| マウス | `LButton` `RButton` `MButton` `X1Button` `X2Button` `WheelUp` `WheelDown` `WheelLeft` `WheelRight` |
| ジェスチャー | `RButton:UL` (ボタン名 `:` 方向 `U`/`D`/`L`/`R` の列) |
| パッド | `Pad.A` `Pad.B` `Pad.X` `Pad.Y` `Pad.LB` `Pad.RB` `Pad.LT` `Pad.RT` `Pad.Back` `Pad.Start` `Pad.LS` `Pad.RS` `Pad.Up` `Pad.Down` `Pad.Left` `Pad.Right` `Pad.LStickUp` など `Pad.[LR]Stick(Up|Down|Left|Right)` |

- Ctrl/Alt/Shift/Win は chord の修飾としてだけ働き、ルールの入力のキーにはできない。
- ホイールとジェスチャーは押しっぱなしにならないので、`press = "hold"` でも tap として扱う。
- パッドの入力は握りつぶせない (元の入力もアプリに届く)。

## 3. 動作

### 3.1 ルールの選び方

キー (修飾キー以外) が押されるたびに、現在の修飾状態からキー列の次の chord を作り、次の条件を満たすルールを宣言順に探す。

1. 現在のモードで有効
2. `targets` のどれかに合う (空なら常に合う)
3. 待機中の chord 列 + 今回の chord が `keys` と一致、または `keys` の接頭辞

完全に一致するルールがあれば (接頭辞としても一致するルールがあっても) すぐに実行する。接頭辞としてだけ一致すれば入力を握りつぶして次を待つ。どちらも無いとき:

- 待機中の chord が無ければ、入力はそのまま流す (モードが `block_unmapped` なら握りつぶす)。
- 待機中の chord があれば、その接頭辞に合っていた最初のルールの `on_mismatch` に従う。
  - `replay`: 待機中の chord と今回の入力をそのまま送り直す
  - `discard`: 待機中の chord と今回の入力を捨てる
  - `fallback`: そのルールの `fallback` を送り、今回の入力は捨てる
- `timeout_ms` を過ぎたときは、今回の入力が無いものとして同じように扱う。タイマーより先に次の入力が来た場合も、先に時間切れを処理してからその入力を新しく扱う。

### 3.2 アクションの実行

`when` が合う最初の実装の手順を実行する。合う実装が無ければルールの `fallback` を送る。呼び出し (`call`) の入れ子は 8 段までとし、超えたらエラーにする。

### 3.3 押しっぱなし (press)

`press = "hold"` で、実装の手順がキー列 1 つだけのとき、最後の chord 以外をタップし、最後の chord は入力を押している間押したままにする (キーボードで打つのと同じ)。同じキーを複数の入力で押しっぱなしにしているときは、最後の入力を離したときに離す。それ以外は `tap` として扱い、押した瞬間に手順を実行する。

キーリピートは物理キーボードと同じく最後のキーだけを繰り返す。hold では押したままのキーの押下だけを送り、修飾キーは押し直さない (リピート中に修飾キーを離せば、キーボードと同じく修飾なしのリピートになる)。tap では最後の入力系の手順 (キー列なら最後の chord、文字、マウス移動) だけを再実行し、それ以外なら何もしない。

### 3.4 修飾キーの退避

出力時、実際に押されている Ctrl/Alt/Shift/Win のうち出力 chord に不要なものは一時的に離し、必要なものは押す。出力の後 (hold なら入力を離した後) に、実際に押されている状態へ戻す。Alt と Win を離す前と押し直した後には、メニューやスタートが開かないように未割り当てのキー (VK 0xE8) を送る。

### 3.5 ユーザー定義修飾キー

押すと保留になり、入力は握りつぶす。保留中に別の入力が押されたら修飾キーとして有効になる。何も押さずに離したら、`tap_timeout_ms` 以内なら `tap` を送る。

`as` を指定した修飾キーを押すと、その修飾キー (例: Ctrl) を実際に押し、離すと離す。ルールに合わない入力はそのまま流れるので、OS からは Ctrl との組み合わせに見える。ルールに合ったときは、出力に不要な修飾キーを一時的に離し (3.4 節)、出力の後で押し直す。単独でタップしたときは、`as` の修飾キーを離してから `tap` を送る。修飾キー自体のキーリピートは、`as` の修飾キーのリピートとして送る (Ctrl を押し続けたのと同じ)。

### 3.6 ジェスチャー

ルールでジェスチャーに使われているマウスボタンを押すと、押下を握りつぶして移動を記録する。移動量が `gesture_threshold` を超えるたびに方向 (直前と違うときだけ) を記録する。ボタンを離したとき、方向が無ければボタン単体の入力として扱い (合うルールが無ければクリックを送り直す)、方向があればジェスチャーとしてルールを探す (合わなければ何もしない)。

### 3.7 一時停止とパススルー

一時停止またはパススルーに入るとき、押しっぱなしの出力を離し、待機中の chord と保留中の修飾キーを捨て、フックを外す。

### 3.8 名前付きパイプ

常駐側は `\\.\pipe\grapnel-<ユーザー名>` で 1 行のコマンドを受け付ける: `reload`, `suspend`, `exit`。
