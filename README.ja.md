# corundum-framework（日本語）

> 言語: [English](README.md) | **日本語**

sapphire の各アプリと asteria が共有するアプリケーションフレームワーク：[egui](https://github.com/emilk/egui) アプリのうち、アプリ固有の画面より手前にあるもの。デスクトップでもスマートフォンでも動きます。

## クレート

| クレート | 内容 | 状態 |
|---|---|---|
| [`corundum-framework-gui`](crates/corundum-framework-gui) | 見た目：シェル、テーマ、フォント、アイコン、翻訳、ウィンドウ | 未リリース |
| `corundum-framework-platform` | egui に依存しない部分：アプリのディレクトリ（モバイルを含む）、システムの言語、JNI／Objective-C との橋渡し | 予定 |
| `corundum-framework` | 上記のクレートをまとめるファサード | 予定 |
| `corundum-framework-demo` | すべてのコンポーネントを見せるデモアプリ | 予定 |

### corundum-framework-gui

- **シェル**：アプリの画面を並べるレール（ウィンドウが狭いとき、たとえばスマートフォンでは下部バー）。設定用とアプリ独自用の枠を持つフッター付き。
- **テーマ**：ライト／ダーク、3段階のコントラスト、アクセントカラー。OKLCH で組み立てます。選ぶための設定ウィジェットと色合いピッカー（パレット、色相／彩度ディスク、プレビュー）。
- **フォント**：システムから CJK のフォールバックフォントを探します（iOS では CoreText、それ以外ではシステムのフォントディレクトリ）。
- **アイコン**：[Lucide](https://lucide.dev) アイコン一式と、それを描く SVG ローダー。
- **翻訳**：すべての文字列を ID で [Fluent](https://projectfluent.org) からユーザーの言語で引きます（`tr!`）。英語と日本語を同梱し、言語ピッカー付き。
- **ウィンドウ**：`window::run` はどのアプリでも同じ方法で wgpu のデスクトップウィンドウを開きます。`window::run_android` は Android 端末（GameActivity）で同じことをします。

| 機能フラグ | 有効にするもの | デフォルト |
|---|---|---|
| `eframe` | デスクトップウィンドウ（`window::run`）とその wgpu 設定（`gpu`） | 有効 |
| `android` | Android のウィンドウ（`window::run_android`）、logcat とプラットフォーム呼び出し | 無効 |
| `serde` | テーマと言語の設定に `Serialize` / `Deserialize` | 無効 |

コンポーネントは egui だけに依存します。eframe を使わずに egui を描くアプリはデフォルト機能を無効にします。Android アプリは `android` を有効にします（eframe のデフォルトのウィンドウは Android ではビルドできません）。

## 状態

未リリースです。最初のリリース 0.1.0 は、デモアプリと各デスクトップ／モバイル OS での手動確認を待っているため、リリース用のワークフローはまだありません。それまでは git 依存として使ってください：

```toml
[dependencies]
corundum-framework-gui = { git = "https://github.com/sapphire-turtle/corundum-framework" }
```

アプリとコンポーネントが同じ egui を使うよう、再エクスポートされた egui（`corundum_framework_gui::egui`）を使ってください。

gui クレートは以前 [sapphire-framework](https://github.com/sapphire-turtle/sapphire-framework) の `crates/corundum-gui` にありました。履歴はそのまま引き継いでおり、履歴中の Issue／プルリクエスト番号は sapphire-framework のものです。

## ライセンス

[Apache License, Version 2.0](LICENSE-APACHE) または [MIT ライセンス](LICENSE-MIT) のいずれかを選択できます。同梱の Lucide アイコンは ISC ライセンス（Feather 由来のものは MIT ライセンス）です（[`crates/corundum-framework-gui/assets/icons/LICENSE-Lucide`](crates/corundum-framework-gui/assets/icons/LICENSE-Lucide)）。
