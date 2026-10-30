# corundum-framework-gui's own strings. Keys are `corundum-<area>-<what>`.

# This language's name, in itself: how the language picker lists it.
corundum-language-name = 日本語
corundum-language = 言語
# The picker's System entry, with the language it resolves to now.
corundum-language-system = システム（{ $language }）

# The theme settings (`theme::settings_ui`).
corundum-theme-appearance = 外観
corundum-theme-light = ライト
corundum-theme-dark = ダーク
corundum-theme-system = システム
corundum-theme-contrast = コントラスト
corundum-theme-low = 低
corundum-theme-medium = 中
corundum-theme-high = 高
# The accent row's label.
corundum-theme-accent = アクセント

# The accent picker (`theme::TintPicker`): the chips' hover names, and the preview's words.
corundum-theme-default = デフォルト
corundum-theme-random = ランダム
corundum-theme-swatch-neutral = ニュートラル
corundum-theme-swatch-red = 赤
corundum-theme-swatch-orange = オレンジ
corundum-theme-swatch-yellow = 黄
corundum-theme-swatch-green = 緑
corundum-theme-swatch-cyan = シアン
corundum-theme-swatch-blue = 青
corundum-theme-swatch-purple = 紫
corundum-theme-link = リンク
corundum-theme-label = ラベル
corundum-theme-selected = 選択中

# The Settings screen (`settings`): the way back to its list on a phone, and the Appearance
# page's heading.
corundum-settings = 設定
corundum-settings-appearance = 外観

# 詳細ペイン（`detail`）：スマートフォンでの戻る、右カラムの切り替え、カードを閉じる。
corundum-detail-back = 戻る
corundum-detail-show = 詳細ペインを表示
corundum-detail-hide = 詳細ペインを隠す
corundum-detail-close = 閉じる

# シェルの「その他」ボタン（`Shell`）。レールに出ない項目があるとき、フッターの位置に出る。
corundum-shell-more = その他

# レールの設定ページ（`settings::RailPage`）：名前、説明、行のハンドルとチェックボックス、既定の順に戻すボタン。
corundum-settings-rail = ナビゲーション
corundum-settings-rail-intro = よく使う順にドラッグで並べ替えます。チェックを外した項目は、ウィンドウに収まらない項目と一緒に ⋯ メニューに入ります。
corundum-settings-rail-drag = ドラッグで並べ替え
corundum-settings-rail-show = ボタンとして表示
corundum-settings-rail-reset = 既定の順に戻す

# 更新パネル（`update::UpdatesPanel`）：もう一度調べるボタン、実行中のバージョンが分からないとき、各行の状態。このアプリが入れないリリースは、入手のしかたを示す：実行するコマンド（コピーボタン付き）か、更新するもの。
corundum-update-check = 更新を確認
corundum-update-not-running = 実行されていません
corundum-update-checking = 確認しています…
corundum-update-up-to-date = 最新です
corundum-update-newer = { $version } があります
corundum-update-update = 更新
corundum-update-release-notes = リリースノート
corundum-update-downloading = { $version } をダウンロードしています…
corundum-update-restarting = 再起動しています…
corundum-update-done = { $version } に更新しました
corundum-update-restart-by-hand = 更新しました。{ $version } を使うには再起動してください
corundum-update-failed = 失敗しました：{ $error }
# バックグラウンドの処理が予期せず止まった行のエラー。
corundum-update-worker-stopped = 更新の確認または更新が予期せず止まりました
corundum-update-retry = 再試行
corundum-update-run = 実行：
corundum-update-copy = コピー
corundum-update-by-cargo = { $tool } で更新してください
corundum-update-by-package-manager = パッケージマネージャーが更新します
corundum-update-by-the-app = アプリ自身が更新します
corundum-update-by-flathub = Flathub が更新します
corundum-update-by-snap = Snap Store が更新します
corundum-update-by-microsoft-store = Microsoft Store が更新します
corundum-update-by-mac-app-store = Mac App Store が更新します
corundum-update-from-release-page = リリースページからダウンロードしてください
# パッケージの行（`update::Target::Package`、`Target::Delegated`）：パッケージは自分で更新し、ダウンロードした更新は再起動で適用する。失敗と再試行は上の行と同じ。
corundum-update-package-current = { $version }
corundum-update-package-uptodate = 最新です（{ $version }）
corundum-update-package-downloading = ダウンロードしています…
corundum-update-package-ready = { $version } の準備ができました
corundum-update-package-restart = 再起動して更新

# アプリメニューの行（`settings::LauncherRow`）：ラベル、二つのボタン、クリックが効かなかった理由。
corundum-launcher-in-menu = アプリメニューに表示
corundum-launcher-remove = 外す
corundum-launcher-add = 戻す
corundum-launcher-failed = アプリメニューを変更できませんでした: { $error }

# ステータスバーの更新アイコン（`update::ReadyBadge`）：ツールチップ。
corundum-update-badge = 再起動して { $version } に更新

# 設定の「バージョン情報」にあるコマンドラインの行（`settings::CliRow`）：ラベル、ボタン、ボタンを出せないときの文、クリックが効かなかった理由。
corundum-cli-row = コマンドライン: { $command }
corundum-cli-install = インストール
corundum-cli-remove = 削除
corundum-cli-repair = 修復
corundum-cli-taken = 別の { $command } がすでにインストールされています。先に削除してください
corundum-cli-not-on-path = ~/.local/bin が PATH にありません
corundum-cli-status-installed = PATH にあります
corundum-cli-status-missing = PATH にありません
corundum-cli-failed = コマンドラインを変更できませんでした: { $error }

# ファイルエクスプローラー（`explorer::FileExplorer`）：ボタン、一覧の見出し、中身がないときの表示、
# 項目のメニュー、複数の項目のドラッグ。
corundum-explorer-up = 上のフォルダーへ
corundum-explorer-refresh = 再読み込み
corundum-explorer-list = 一覧
corundum-explorer-grid = グリッド
corundum-explorer-name = 名前
corundum-explorer-size = サイズ
corundum-explorer-modified = 更新日時
corundum-explorer-loading = 読み込み中…
corundum-explorer-empty = このフォルダーは空です
corundum-explorer-failed = このフォルダーを読み込めませんでした: { $error }
corundum-explorer-retry = 再試行
corundum-explorer-open = 開く
corundum-explorer-rename = 名前を変更
corundum-explorer-items = { $count } 項目

# デスクトップで写真や動画を選ぶダイアログ（`platform::MediaPicker`）：フィルタ。
corundum-media-images-and-videos = 画像と動画
corundum-media-images = 画像
corundum-media-videos = 動画

# QR コード（`qr::QrCode`）：文字列が長すぎて QR コードにできないとき。
corundum-qr-too-long = 長すぎて QR コードにできません。

# 初回起動の流れ（`first_run::FirstRun`）: 見出しのステップ表示と、下部のボタン。
corundum-first-run-step = ステップ { $step } / { $count }
corundum-first-run-back = 戻る
corundum-first-run-next = 次へ
corundum-first-run-finish = 完了
