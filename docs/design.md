# grapnel 設計書

## 1. crate 構成

```
crates/
  grapnel-keys      キー名・chord・キー列の型と解析 (no deps, wasm 可)
  grapnel-schema    設定ファイルの serde 型 (serde のみ, wasm 可)
  grapnel-config    読み込み・include・検証・コンパイル, ターゲット照合 (regex)
  grapnel-engine    変換エンジン。入力イベント → 出力コマンドの純粋な状態機械
  grapnel-pad       gilrs でパッド入力を Key の押下/解放に変換する
  grapnel-win       Win32: フック, SendInput, 前面ウインドウ/UIA, トレイ, 入力欄, パイプ
  grapnel           常駐 exe。上記を配線する
apps/settings/
  src-tauri         grapnel-settings (Tauri 2)。読込/検証/保存/適用のコマンド
  ui                Leptos (CSR, trunk)。フォーム UI
```

依存の向き:

```
keys ← schema ← config ← engine ← grapnel(bin) → win, pad
keys, schema ← ui          keys, schema, config ← src-tauri
```

`cargo test` は既定メンバー (crates/*) だけを対象にする。Tauri 側は `cargo tauri build` で別に作る。

## 2. grapnel-keys

```rust
pub enum Key {
    Vk(u8), Sc(u16),                  // キーボード
    Mouse(MouseButton), Wheel(WheelDir),
    Gesture(MouseButton, Vec<Dir>),
    Pad(PadButton),
}
pub struct Mods { pub ctrl, alt, shift, win: bool, pub user: u32 /* ビット集合 */ }
pub struct Chord { pub mods: Mods, pub key: Key }
pub struct KeySeq(pub Vec<Chord>);
```

- 解析は `parse_seq(s, &user_mod_names)` で、ユーザー修飾名の表を受け取ってビット番号へ変換する。
- `Display` で正規形の文字列に戻せる (設定ツールでの表示と往復テストに使う)。
- VK の名前表はこの crate に持つ。スキャンコードとの変換は OS に頼る (grapnel-win)。

## 3. grapnel-schema

TOML と 1 対 1 の serde 型 (`RawConfig`, `RawRule`, `RawAction`, `RawStep` ...)。
意味の検証はしない。設定ツールのフロントエンド (wasm) とバックエンドで共有する。
`RawStep` は `#[serde(untagged)]` で文字列 (= keys) とテーブルの両方を受ける。

## 4. grapnel-config

```rust
pub fn load(entry: &Path) -> Result<Vec<(PathBuf, RawConfig)>, Vec<Error>>   // include 展開
pub fn compile(files: &[(PathBuf, RawConfig)]) -> Result<Config, Vec<Error>>
pub fn save(path: &Path, raw: &RawConfig) -> io::Result<()>
```

- `Config` はエンジンが使う形: 名前を添字に解決済み (`TargetId`, `ActionId`, `ModeId`)。
- ターゲットは `Matcher` (Exact / Regex) に変換する。glob は正規表現へ変換する。
- `WindowInfo { exe_name, exe_path, title, class, control, uia_id, uia_name, uia_type }` と `Config::target_matches(id, &WindowInfo)` を提供する。
- エラーはファイル名と位置 (例 `rules[3].keys`) を含む文字列で、見つかったものをすべて返す。
- include の glob は最後のパス要素にだけワイルドカードを許す。

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
    ModeChanged(String), Control(Control),
}
impl Engine {
    pub fn new(cfg: Arc<Config>) -> Self;
    pub fn handle(&mut self, ev: Event, win: &WindowInfo, now: u64) -> Reaction;
    pub fn tick(&mut self, win: &WindowInfo, now: u64) -> Vec<Command>;   // chord の時間切れ
    pub fn next_deadline(&self) -> Option<u64>;
    pub fn invoke(&mut self, action: ActionId, arg: &str, win: &WindowInfo) -> Vec<Command>;
    pub fn reset(&mut self) -> Vec<Command>;                                // 押しっぱなしを解放
}
```

状態:

| 状態 | 内容 |
| --- | --- |
| `down` | 物理的に押されているキーの集合 (リピート判定と修飾状態) |
| `os_mods` | OS から見て押されている修飾キー (退避と復元の基準) |
| `pending` | 待機中の chord 列と最初に合った接頭辞ルール、期限 |
| `user_mods` | ユーザー修飾キーの状態 (保留 / 有効) |
| `holds` | 入力キー → 離したときに送るコマンド |
| `swallowed` | 押下を握りつぶしたキー。解放も握りつぶす |
| `gesture` | 記録中のボタン、移動量の累計、方向列 |
| `mode` | 現在のモード |

ジェスチャーでボタン単体として扱う場合や `replay` では、握りつぶした入力を `Command::Key` で送り直す。

## 6. grapnel-win

| モジュール | 内容 |
| --- | --- |
| `hook` | LL フックの設置/解除。コールバックで `Event` に変換してクロージャを呼び、戻り値で握りつぶす。`dwExtraInfo` が自分の印なら素通し |
| `send` | `Command::Key/Text/MouseMove` を `INPUT` 配列に変換して `SendInput`。変換部は純粋関数でテストする |
| `window` | `SetWinEventHook` (前面切替・タイトル変更・フォーカス) で `WindowInfo` を更新する。UIA はワーカースレッドで取得して結果をメッセージで返す |
| `tray` | `Shell_NotifyIconW`、メニュー、バルーン |
| `inputbox` | Edit を 1 つ持つポップアップ。Enter で確定、Esc で取消 |
| `pipe` | 名前付きパイプのサーバースレッド。受けたコマンドをメインスレッドへ `PostMessage` |
| `notify` | `MessageBoxW` |

## 7. 常駐 exe (grapnel)

- メインスレッドにメッセージ専用ウインドウを作り、フック・WinEvent・タイマー・トレイ・ホットキー・パイプ・パッドの通知をすべてここで受ける。エンジンはこのスレッドだけが触るので同期は不要。
- パッドは gilrs のスレッドでポーリングし、`Down/Up` を `PostMessage` で送る。
- コマンドの実行: 最初の `Sleep` までは呼び出し元 (フック内) で同期的に送る。`Sleep` 以降はワーカースレッドへ渡して順に実行する。フック内で同期的に送るのは、後続の物理入力より先に出力を届けるため。
- `Run` は `std::process::Command::spawn`、`InputBox` はポップアップを出し、確定時に `Engine::invoke`。
- タイマー: `Engine::next_deadline` から `SetTimer` を張り直す。
- パススルー: 前面が変わるたびに `settings.passthrough` を評価し、入ったら `Engine::reset` → フック解除、出たらフック再設置。
- 再読み込み: `load` → `compile` に成功したら `Engine::reset` して新しいエンジンに差し替える。失敗したらバルーンで知らせて旧設定を維持する。
- ログ: `log` crate。デバッグでは stdout、リリースではファイルへ書き、`error` はバルーンも出す。

## 8. 設定ツール (grapnel-settings)

- Tauri コマンド: `load(entry?) -> Vec<FileDoc>`, `validate(files) -> Vec<String>`, `save(files) -> Result`, `apply() -> Result`。`FileDoc = { path, raw: RawConfig }`。
- `apply` はパイプに `reload` を書く (`std::fs::OpenOptions` でパイプを開ける)。
- UI: 左にファイル一覧、右にセクションのタブ (settings / modes / modifiers / targets / rules / actions)。各セクションは行の一覧と追加・削除ボタン。キー欄は入力のたびに `grapnel-keys` で解析してエラーを出す。
- 保存は `validate` が空のときだけ許す。

## 9. テスト方針

- keys / schema / config / engine は単体テストで TDD。engine は `Event` 列 → `Reaction` 列のシナリオテストで仕様書 3 章の各項目を検証する。
- win は `send` の変換部だけ単体テストし、残りは手動で確認する。
