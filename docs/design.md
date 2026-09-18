# grapnel 設計書

## 1. crate 構成

```
crates/
  grapnel-keys      キー名・chord・キー列の型と解析 (依存なし, wasm 可)
  grapnel-schema    設定ファイルの serde 型 (serde のみ, wasm 可)
  grapnel-config    読み込み・include・検証・コンパイル, ターゲット照合 (regex)
  grapnel-engine    変換エンジン。入力イベント → 出力コマンドの純粋な状態機械
  grapnel-pad       gilrs でパッド入力をボタンの押下/解放に変換する
  grapnel-win       Win32: フック, SendInput, 前面ウインドウ/UIA, トレイ, 入力欄, パイプ
apps/grapnel/       常駐 exe。上記を配線する
apps/settings/      grapnel-settings-ui: Leptos (CSR, trunk) のフロントエンド
  src-tauri/        grapnel-settings: Tauri 2 のバックエンド (設定ツールの exe)
```

依存の向き:

```
keys ← schema ← config ← engine ← win ← grapnel(bin) → pad
keys, schema ← settings-ui          schema, config, win ← settings (Tauri)
```

`cargo test` は既定メンバー (crates/* と apps/grapnel) だけを対象にする。ライブラリは crates/、実行ファイルは apps/ に置く。設定ツールは `apps/settings` で `cargo tauri build` する。

## 2. grapnel-keys

```rust
pub enum Key {
    Vk(u8), Sc(u16),                  // キーボード (Sc は 0xE0xx で拡張キー)
    Mouse(MouseButton), Wheel(WheelDir),
    Gesture(MouseButton, Vec<Dir>),
    Pad(PadButton),
}
pub struct Mods(pub u32);             // bit0-3 = C/M/S/W, bit4 以降 = ユーザー修飾キー (最大 28)
pub struct Chord { pub mods: Mods, pub key: Key }
pub struct KeySeq(pub Vec<Chord>);
pub fn parse_seq(s: &str, user_mods: &[&str]) -> Result<KeySeq, String>;
pub fn format_seq(seq: &KeySeq, user_mods: &[&str]) -> String;
```

- ユーザー修飾キーの名前はビット番号で表すので、解析と整形には名前の表を渡す。
- 半角/全角・かな・英数は IME の状態で VK が変わるため、スキャンコードの名前として定義する。

## 3. grapnel-schema

TOML と 1 対 1 の serde 型 (`RawConfig`, `RawNode`, `RawBinding`, `RawAction`, `RawStep` ...)。
意味の検証はしない。設定ツールのフロントエンド (wasm) とバックエンドで共有する。

- キーマップの節 `RawNode` は `options` と、`#[serde(flatten)]` した chord → `RawBinding` の `IndexMap`。
- `RawBinding` は untagged で、文字列 / 手順の配列 / `do` を持つテーブル (`RawLeaf`) / それ以外のテーブル (子の節) の順に試す。`RawLeaf` は未知の項目を `unknown` に集めて compile 側でエラーにする (綴り間違いが子の節と解釈されるのを防ぐ。flatten があると配列からは読めなくなる効果もある)。
- アクション `RawAction` は文字列 / 手順の配列 / ターゲット名 → 手順の `IndexMap`。評価順が意味を持つので、マップは `IndexMap` で順序を保つ (`toml` の `preserve_order` を使う)。

## 4. grapnel-config

```rust
pub fn load(entry: &Path) -> Result<Vec<(PathBuf, RawConfig)>, Vec<String>>   // include 展開
pub fn compile(files: &[(PathBuf, RawConfig)]) -> Result<Config, Vec<String>>
pub fn save(path: &Path, raw: &RawConfig) -> io::Result<()>
pub fn any_matches(targets: &[Target], ids: &[TargetId], win: &WindowInfo) -> bool
```

- `Config` はエンジンが使う形で、名前は添字 (`TargetId`, `ActionId`, `ModeId`) に解決済み。
- キーマップの木 (`keymap.rs`) は、葉ごとの `Rule` (キー列・アクション・モード・press など) に平らにする。モード専用のキーマップの `Rule` を先に並べるので、エンジンは宣言順に探すだけで優先順位が付く。
- 節の `options` は祖先の値と合わせて `Prefix { keys, modes, policy }` にする。エンジンは待機中の chord 列に対して `Config::policy` で一番深い節の設定を引く。
- キーマップにその場で書いたアクション (文字列のキー列、手順の配列) は、位置を名前にした無名のアクションとして `actions` の後ろに足す。
- 同じモードでの重複キーと、短いキーに隠れて届かないキーはエラーにする。
- ターゲットの文字列は `Matcher` (Exact / Regex) に変換する。glob は正規表現に変換する。
- エラーは `ファイル: 位置: 内容` (例 `main.toml: keymap."C-x".t: unknown key 't'`) の形で、見つかったものをすべて返す。
- include の glob は最後のパス要素にだけワイルドカードを許す。
- `Config::scancodes()` はルールと修飾キーで使うスキャンコードの一覧。フックはこれに含まれるキーだけ `Key::Sc` で報告する。

## 5. grapnel-engine

OS に依存しない状態機械。時刻はミリ秒の `u64` で外から渡す。

```rust
pub enum Event { Down(Key), Up(Key), MouseMove { dx: i32, dy: i32 } }
pub struct Reaction { pub consume: bool, pub commands: Vec<Command> }
pub enum Command {
    Key { key: Key, down: bool },          // マウスボタン・ホイールも含む
    Text(String), MouseMove { x: i32, y: i32, absolute: bool },
    Sleep(u32), Run { program: String, args: Vec<String> },
    InputBox { prompt: String, then: ActionId },
    ModeChanged(String), Control(ControlCmd), Error(String),
}
impl Engine {
    pub fn new(cfg: Arc<Config>) -> Self;
    pub fn handle(&mut self, ev: Event, win: &WindowInfo, now: u64) -> Reaction;
    pub fn tick(&mut self, now: u64) -> Vec<Command>;                      // chord の時間切れ
    pub fn next_deadline(&self) -> Option<u64>;
    pub fn invoke(&mut self, action: ActionId, arg: &str, win: &WindowInfo) -> Vec<Command>;
    pub fn reset(&mut self) -> Vec<Command>;                                // 押しっぱなしを解放
    pub fn sync_modifiers(&mut self, held: &[Key]);                         // フック再設置時など
}
```

状態:

| 状態 | 内容 |
| --- | --- |
| `down` | 物理的に押されているキーの集合 (リピート判定と chord の修飾状態) |
| `os_mods` | OS から見て押されている修飾キー (退避と復元の基準) |
| `pending` | 待機中の chord 列と、その節の設定 (`Policy`)、期限 |
| `user_mods` | ユーザー修飾キーの状態 (待機 / 保留 / 有効) |
| `active` | 押されている入力キー → `Hold(出力 chord)` / `Pass(元のキー)` / `Tap(リピート時に再実行する手順)` |
| `swallowed` | 押下を握りつぶしたキー。解放も握りつぶす |
| `gesture` | 記録中のボタン、移動量の累計、方向列 |
| `mode` | 現在のモード |

- リピートでは保存したコマンドを再送せず、その時点の修飾状態から出力を作り直す。
- 素通しにしたい入力を一度握りつぶした場合 (replay、既定の fallback、時間切れ後の入力) は、`Pass` として注入し、解放まで追跡する。
- ソースは `lib.rs` (型と公開 API)、`input.rs` (入力処理と照合)、`output.rs` (出力生成と修飾キーの退避) に分ける。

## 6. grapnel-win

| モジュール | 内容 |
| --- | --- |
| `hook` | LL フックの設置/解除。コールバックで `Event` に変換してスレッドローカルのハンドラを呼び、戻り値で握りつぶす。`dwExtraInfo` が自分の印 (`INJECT_TAG`) なら素通し |
| `send` | `Command::Key/Text/MouseMove` を `INPUT` 配列に変換して `SendInput`。変換部は純粋関数でテストする |
| `window` | `SetWinEventHook` (前面切替・タイトル変更・フォーカス) の通知と、前面ウインドウの `WindowInfo` 取得 |
| `uia` | UI Automation の問い合わせを MTA のワーカースレッドで行い、結果の準備ができたら通知する |
| `tray` | `Shell_NotifyIconW`、バルーン、メニュー (メニューはモーダルループを回すので自由関数) |
| `toast` | モニターの左下か右下に短いメッセージを出す (Emacs のエコーエリア風)。フォーカスを奪わず、クリックを透過し、タイマーで消える。`Command::Notice` (左下) と実行中のエラー (右下) の表示に使う |
| `inputbox` | Edit を 1 つ持つポップアップ。Enter で確定、Esc で取消。前面化は Alt の注入でロックを外す (`AttachThreadInput` は相手のハングに巻き込まれるので使わない)。閉じたら元の前面ウインドウに戻す |
| `pipe` | 名前付きパイプのサーバースレッド。最初のインスタンスの作成に失敗したら多重起動とみなす。クライアント側は混雑時に再試行する |

## 7. 常駐 exe (grapnel)

- メインスレッドに非表示のウインドウを作り、フック・WinEvent・タイマー・トレイ・ホットキーをすべてここで受ける。エンジンはこのスレッドの `App` (スレッドローカルの `RefCell`) だけが触る。
- 他スレッド (パイプ、パッド、ワーカー、ログ) からの仕事は、プロセス内のチャネル (`mpsc::channel`) に送り、`WM_QUEUE` で起こして処理する。メッセージの引数にポインタは載せない (他プロセスから偽装されうるため)。
- コマンドの実行 (`exec.rs`):
  - 最初の `Sleep` までは呼び出し元 (フック内) で順に送る。後続の物理入力より先に出力を届けるため。
  - `Sleep` 以降はワーカースレッドで実行する。一時停止・パススルー・再読み込み・終了で世代番号を進め、未実行の分は捨てる。
  - `Run` は起動用のスレッドを立てて `std::process::Command::spawn` する (フックのスレッドを止めない)。
  - それ以外 (`InputBox`、`ModeChanged`、`Control`、`Error`) はキュー経由でメインスレッドが処理する。入力欄はウインドウを作る際にメッセージが回るので、`App` を借用していない状態で開く。
- 入力欄が前面にある間は、修飾キー以外の入力を変換しない (修飾キーの状態だけ追跡する)。開く前にエンジンを `reset` し、開いたときの `WindowInfo` で後続のアクションを選ぶ。
- 一時停止のホットキーは、ルールやモードに握りつぶされないようフックの段階で素通しにする。
- タイマー: `Engine::next_deadline` から `SetTimer` を張り直す。
- パススルー: 前面の変化 (UIA を使う設定では UIA の結果が届いた時点) で `settings.passthrough` を評価し、入ったら `Engine::reset` → フック解除、出たらフック再設置 → `GetAsyncKeyState` で修飾キーを同期。
- 再読み込み: 読み込みとコンパイルはワーカースレッドで行い、結果をキューで受け取る。成功したら新しいエンジンに差し替えて修飾キーを同期し、失敗したらバルーンで知らせて旧設定を維持する。
- ログ: `log` crate。デバッグでは stdout、リリースでは `%LOCALAPPDATA%\grapnel\grapnel.log` へ書き、`error` は右下のトーストにも出す (設定の再読み込み結果はバルーン)。
- 起動引数: `--config <path>`。`grapnel reload|suspend|exit` で起動中のインスタンスをパイプ経由で操作する。

## 8. 設定ツール (grapnel-settings)

- Tauri コマンド: `initial_entry()`, `load(entry) -> Vec<FileDoc>`, `validate(files) -> Vec<String>`, `save(files)`, `apply()`。`FileDoc = { path, raw: RawConfig }`。
- `save` は全ファイルを検証してから書く。`apply` はパイプに `reload` を書く。
- フロントエンドは引数を `serde_json` で JSON 文字列にしてから `JSON.parse` で JS のオブジェクトにして渡す (マップを `Map` にせず、`__proto__` のようなキーも失わないため)。
- UI: 上にエントリのパスと読み込み・保存・保存して適用、ファイルのタブ、セクションのタブ (全般 / モード / 修飾キー / ターゲット / キーマップ / アクション)、検証エラーの一覧。編集のたびに検証し、エラーがあれば保存ボタンを無効にする。
- キーマップは再帰的な節エディタで編集する。各キーの種類 (キー・アクション名 / 手順 / 設定付き / 子の節) を切り替えると、残せる内容は残して変換する。モードのタブでは、同じ節エディタでモード専用のキーマップを編集する。
- Tauri はコマンドの引数を `serde_json::Value` 経由で読むので、バックエンドで `serde_json` の `preserve_order` を有効にしてキーの順序を保つ (アクションのターゲットは順序が意味を持つ)。
- 値の入出力は `Place` (現在のファイル内の 1 か所を指す読み書きの組) とフィールド関数 (`text`, `opt_text`, `list`, `num`, `keys`, `check`, `select`) で束ねる。入力は `change` イベントで反映する (打鍵ごとの再描画でフォーカスを失わないため)。
- キー欄は `grapnel-keys` でその場で構文を検証する。

## 9. テスト方針

- keys / schema / config / engine は単体テストで TDD。engine は `Event` 列 → `Reaction` 列のシナリオテストで仕様書 3 章の各項目を検証する (`tests/basic.rs`, `tests/advanced.rs`, `tests/review.rs`)。
- win は `send` の変換部、マウスメッセージの変換、パイプの送受信を単体テストし、残りは手動で確認する。
- pad はスティックのヒステリシスと複数台の合成・切断を単体テストする。
