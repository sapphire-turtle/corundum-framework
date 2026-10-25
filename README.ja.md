# corundum-framework（日本語）

> 言語: [English](README.md) | **日本語**

sapphire の各アプリと asteria が共有するアプリケーションフレームワーク：[egui](https://github.com/emilk/egui) アプリのうち、アプリ固有の画面より手前にあるもの。デスクトップでもスマートフォンでも動きます。

## クレート

| クレート | 内容 | 状態 |
|---|---|---|
| [`corundum-framework-gui`](crates/corundum-framework-gui) | 見た目：シェル、テーマ、フォント、アイコン、翻訳、ウィンドウ | 未リリース |
| [`corundum-framework-platform`](crates/corundum-framework-platform) | egui に依存しない部分：アプリのディレクトリ（モバイルを含む）、システムの言語、JNI／Objective-C との橋渡し | 未リリース |
| [`corundum-framework`](crates/corundum-framework) | 上記のクレートをまとめるファサード | 未リリース |
| `corundum-framework-demo` | すべてのコンポーネントを見せるデモアプリ | 予定 |

### corundum-framework-gui

- **シェル**：アプリの画面を並べるレール（ウィンドウが狭いとき、たとえばスマートフォンでは下部バー）。設定用とアプリ独自用の枠を持つフッター付き。広い画面では下端にステータスバーを出し、左側はアプリが自由に使え（ログの1行、サービスの状態、再生中の曲など）、右端にアプリ独自の枠と詳細ペインの切り替えボタンを置きます。
- **テーマ**：ライト／ダーク、3段階のコントラスト、アクセントカラー。OKLCH で組み立てます。選ぶための設定ウィジェットと色合いピッカー（パレット、色相／彩度ディスク、プレビュー）。
- **フォント**：システムから CJK のフォールバックフォントを探します（iOS では CoreText、それ以外ではシステムのフォントディレクトリ）。
- **アイコン**：[Lucide](https://lucide.dev) アイコン一式と、それを描く SVG ローダー。
- **翻訳**：すべての文字列を ID で [Fluent](https://projectfluent.org) からユーザーの言語で引きます（`tr!`）。英語と日本語を同梱し、言語ピッカー付き。
- **設定**：設定画面。ページの一覧と開いたページを並べ、アプリやその下のフレームワークがページを差し込みます。設定を保存できる場所（デバイスとワークグループ、クライアントとサーバーなど）ごとのスコープのタブと、それを使ってテーマと言語を設定する外観ページもあります。ページは値を保存しません。保存はアプリが行います。
- **詳細ペイン**：メインビューで選んだもの（再生中のプレイリスト、開いているジャーナルなど）の詳細。広い画面では右カラムに表示し、ステータスバーのボタンで隠せます。隠しているときは画面中央のカードに、スマートフォンではメインビューと切り替えて表示します。
- **ウィンドウ**：`window::run` はどのアプリでも同じ方法で wgpu のデスクトップウィンドウを開きます。`window::run_android` は Android 端末（GameActivity）で同じことをします。

| 機能フラグ | 有効にするもの | デフォルト |
|---|---|---|
| `eframe` | デスクトップウィンドウ（`window::run`）とその wgpu 設定（`gpu`） | 有効 |
| `android` | Android のウィンドウ（`window::run_android`）、logcat とプラットフォーム呼び出し | 無効 |
| `serde` | テーマと言語の設定に `Serialize` / `Deserialize` | 無効 |

コンポーネントは egui だけに依存します。eframe を使わずに egui を描くアプリはデフォルト機能を無効にします。Android アプリは `android` を有効にします（eframe のデフォルトのウィンドウは Android ではビルドできません）。

### corundum-framework-platform

アプリが OS に求めるもののうち、egui に依存しない部分です。ウィンドウを持たないクレート（サーバー、CLI）でも使えます。アプリが JNI や Objective-C を直接呼ばずに済むようにします。

- **ディレクトリ**：`dirs::{config_dir, data_dir, cache_dir}` と、その下のアプリごとの `AppDirs`。デスクトップでは `dirs` クレートのもの、iOS（`NSFileManager`）と Android（Context のファイル・キャッシュのディレクトリ）ではサンドボックスのもの。`AppDirs` はデスクトップでは `<root>/<アプリ名>`、スマートフォンではサンドボックスのディレクトリそのもの。
- **言語**：`locale::system_languages`。システムの言語の優先順。Android では `LocaleList` 全体を読みます（`sys-locale` は一つしか返しません）。
- **ウィンドウの補助**：Android のクリップボードとウィンドウのインセット。gui クレートの `platform` がこれを読みます。
- **Android**：`android::with_env`（Java の例外を捕まえて JavaVM にアタッチする）と、アプリケーションの Context とアクティビティ。アプリ独自の JNI 呼び出しに使います。

### corundum-framework

上記のクレートを一つの依存で使うためのファサードです。sapphire-framework のファサードと同じ形です。feature ごとにクレートをモジュールとして再公開し（`corundum_framework::platform`、`::gui`）、残りの feature は gui クレートに渡します。

| feature | 有効になるもの | 既定 |
|---|---|---|
| `platform` | `platform`：corundum-framework-platform | はい |
| `gui` | `gui` と `egui`：eframe を含まない corundum-framework-gui（`platform` も有効になる） | いいえ |
| `eframe` | `gui::window::run`：デスクトップのウィンドウ（`gui` も有効になる） | いいえ |
| `android` | `gui::window::run_android`：Android のウィンドウ（`gui` も有効になる） | いいえ |
| `serde` | gui クレートの `serde` | いいえ |

既定では egui を含みません。サーバーや CLI はそのまま使えます。デスクトップのアプリは `eframe` を、Android のアプリは `android` を有効にします。

## デモ

[`demo/`](demo/README.md) は、共通の機能（シェル、テーマ、フォント、アイコン、翻訳、プラットフォームの呼び出し）を一通り触れるアプリです。アプリと切り離して各 OS で確かめるために使います。デスクトップでは `cargo run -p corundum-framework-demo` で起動し、iOS シミュレーターと Android エミュレーター向けには Xcode と Gradle のプロジェクトがあります。公開はしません。

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
