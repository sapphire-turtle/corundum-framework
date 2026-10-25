# デモ自身の文字列。キーは `demo-<画面>-<内容>`。テーマと言語の設定は corundum-framework-gui
# の翻訳を使う。

demo-title = corundum-framework デモ

# 画面。レールのホバーで出る名前。
demo-screen-overview = 概要
demo-screen-theme = テーマ
demo-screen-fonts = フォント
demo-screen-icons = アイコン
demo-screen-language = 言語
demo-screen-detail = 詳細
demo-screen-platform = プラットフォーム
demo-screen-settings = 設定
# レールのアクション。クリックの回数を概要に出す。
demo-action-count = クリックを数える
# フッターのスロットの点（ホバー）。
demo-foot-online = オンライン（クリックでオフライン）
demo-foot-offline = オフライン（クリックでオンライン）

demo-overview-intro = どの画面も、アプリと同じように corundum-framework を使っています。ウィンドウの幅を { $width } ポイントより狭くすると、スマートフォンの配置になります。
demo-overview-layout = 配置
demo-overview-rail = 左のレール（幅 { $width } pt）
demo-overview-bar = 下部のバー（幅 { $width } pt）
demo-overview-clicks = レールのアクションを { $count } 回クリックしました。
demo-overview-online = フッターのスロットの点は「オンライン」です。
demo-overview-offline = フッターのスロットの点は「オフライン」です。
demo-overview-open-settings = アプリから設定を開く

demo-theme-settings = テーマの設定
demo-theme-picker = TintPicker 単体
demo-theme-palette = パレット（トーン × スウォッチ）
demo-theme-widgets = このテーマでのウィジェット
demo-theme-button = ボタン
demo-theme-disabled = 無効
demo-theme-checkbox = チェックボックス
demo-theme-selectable = 選択できる項目
demo-theme-link = リンク
demo-theme-hint = テキスト欄

demo-fonts-fallback = CJK のフォールバック：{ $family }
demo-fonts-none = CJK のフォントが見つかりません。CJK の文字は四角で表示されます
demo-fonts-proportional = プロポーショナル
demo-fonts-monospace = 等幅

demo-icons-intro = 同梱の Lucide アイコン { $count } 個。表示：
demo-icons-text = 文字色
demo-icons-muted = 控えめ
demo-icons-accent = アクセント
demo-icons-red = 赤
demo-icons-green = 緑
demo-icons-button = ボタン
demo-icons-selected = 選択中
demo-icons-disabled = 無効

demo-language-picker = 言語の選択
demo-language-current = 表示中：{ $language }
demo-language-available = 選べる言語：{ $languages }
demo-language-system = システムの言語：{ $languages }
demo-language-greeting = こんにちは。この文は、デモ自身の翻訳から出しています。
demo-language-clicks = これまでのクリック：{ $count } 回

demo-platform-system = システム
demo-platform-os = { $os }（{ $arch }）
demo-platform-clipboard = クリップボード
demo-platform-sample = corundum-framework のデモからコピーしました
demo-platform-copy = 見本の文をコピー
demo-platform-paste = 貼り付け
demo-platform-paste-here = ここに貼り付け
demo-platform-read = システムのクリップボードを読む
demo-platform-clipboard-text = システムのクリップボード：{ $text }
demo-platform-clipboard-none = 読めませんでした。ここでは egui のクリップボードを使います。
demo-platform-insets = インセット
demo-platform-safe-area = セーフエリア（egui、ポイント）：{ $insets }
demo-platform-window-insets = ウィンドウのインセット（Android、ピクセル）：{ $insets }
demo-platform-viewport = ビューポート：{ $rect }
demo-platform-content = コンテンツ：{ $rect }
demo-platform-ppp = 1 ポイントのピクセル数：{ $ppp }
demo-platform-keyboard = ここに入力するとキーボードが開きます
demo-platform-dirs = アプリのディレクトリ
demo-platform-config = 設定：{ $path }
demo-platform-data = データ：{ $path }
demo-platform-cache = キャッシュ：{ $path }
demo-platform-none = なし
demo-platform-refresh = 読み直す

# 設定画面：corundum のもので、デモのページを差し込んだもの。
demo-settings-demo = デモ
demo-settings-sample = サンプル
demo-settings-sample-intro = アプリ独自のページです。変更は「適用」で反映され、それまでは一覧の項目に印が付きます。
demo-settings-sample-name = 名前
demo-settings-sample-saved = 保存済み：{ $name }
demo-settings-apply = 適用
demo-settings-revert = 元に戻す
demo-settings-general = 一般
# 外観を保存する場所：このデバイスのファイル（corundum の store）。レールの並び順も一緒に保存する。
demo-settings-scope-device = このデバイス
demo-settings-look-file = レールの並び順と一緒に { $path } に保存し、次に起動したときも使います。
demo-settings-look-memory = デモの実行中だけ保持します。このプラットフォームには設定ディレクトリがありません。
demo-settings-about = このアプリについて
demo-settings-version = バージョン { $version }

# 詳細画面：アルバムと、選んだアルバムの曲を詳細ペインに。
demo-detail-intro = アルバムを選ぶと曲が表示されます。広い画面では右カラムに表示され、ステータスバー右端のボタンで隠せます。隠しているときやスマートフォンでは、アルバムを選ぶと開きます。
demo-detail-tracks = { $count } 曲

# ステータスバーの1行：最後に起きたこと。
demo-status-ready = 準備完了
demo-status-counted = { $count } まで数えました
demo-status-picked = { $album } を選びました
