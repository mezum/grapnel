# grapnel

キーボード・マウス・ゲームパッドの入力を別の入力に変換する Windows 11 用の常駐ツールです。

- Emacs 風の chord (`C-x t 0`)、Vim 風のモード、tap/hold 兼用のユーザー定義修飾キー
- マウスのボタン・ホイール・ジェスチャー、ゲームパッドのボタン・スティックも入力にできる
- アプリ・ウインドウ・コントロール (UI Automation を含む) ごとにルールとアクションを切り替える
- 指定したアプリが前面にある間はフックを外す (パススルー)
- 設定は TOML。GUI の設定ツール (`grapnel-settings`) でも編集できる

文書: [要件定義書](docs/requirements.md) / [仕様書 (設定の書き方)](docs/spec.md) / [設計書](docs/design.md)

## ビルド

必要なもの: Rust stable (rust-toolchain.toml で固定)、`wasm32-unknown-unknown` ターゲット、[trunk](https://trunkrs.dev/)、[tauri-cli](https://tauri.app/) 2.x。

```sh
cargo build --release                 # 常駐側 target/release/grapnel.exe
cargo test                            # コア crate のテスト
cd apps/settings && cargo tauri build --no-bundle   # 設定ツール target/release/grapnel-settings.exe
```

設定ツールの開発時は `apps/settings` で `cargo tauri dev` を使います。

## 使い方

`grapnel.exe` を起動するとタスクトレイに常駐します。設定ファイルは既定で `%APPDATA%\grapnel\config.toml` で、無ければ空のファイルを作ります。

```sh
grapnel.exe --config D:\path\to\config.toml   # 設定ファイルを指定して起動
grapnel.exe reload                            # 起動中のインスタンスに再読み込みを指示
grapnel.exe suspend                           # 一時停止を切り替え
grapnel.exe exit                              # 終了
```

トレイアイコンのメニューから、一時停止・再読み込み・設定ツールの起動・終了ができます。
設定ツールは `grapnel.exe` と同じフォルダに `grapnel-settings.exe` を置くと、メニューから開けます。

最小の設定例:

```toml
[modifiers.Mu]
key = "Muhenkan"          # 無変換: 単独なら無変換、押しながらなら修飾キー

[keymap]
"Mu-h" = "Left"           # 無変換+h で ←
"C-x C-s" = "C-s"         # 続けて押すキー列 (C-x を押してから C-s)
```

書き方の詳細は [仕様書](docs/spec.md) を、Emacs 風キーバインドの例は [examples/emacs.toml](examples/emacs.toml) を、F19 を macOS の Cmd キーのように使う例は [examples/mac-cmd.toml](examples/mac-cmd.toml) を、Vim 風のモード (normal / input / visual / command / search) の例は [examples/vim.toml](examples/vim.toml) を、JIS 配列のキーボードで AX 配列の記号を打つ例は [examples/ax.toml](examples/ax.toml) を参照してください。ax.toml 以外は共通のアクションを [examples/default-actions.toml](examples/default-actions.toml) から読み込みます。

## 制限

- どのキーボード/マウスから来た入力かは区別しません。
- ゲームパッドへの出力には対応していません。パッドの元の入力はアプリにも届きます。
- 管理者権限で動いているアプリの上で変換するには、grapnel も管理者として起動する必要があります。
- 設定ツールで保存すると、ファイル内のコメントは消えます。

## ライセンス

[MIT](LICENSE-MIT) または [Apache-2.0](LICENSE-APACHE) のどちらかを選べます。同梱している他者の素材は [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) を参照してください。
