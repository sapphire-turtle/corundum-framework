# The demo's own strings. Keys are `demo-<screen>-<what>`; corundum-framework-gui's own
# (the theme and language settings) come from its bundle.

demo-title = corundum-framework demo

# The screens, as the rail's hover text names them.
demo-screen-overview = Overview
demo-screen-theme = Theme
demo-screen-fonts = Fonts
demo-screen-icons = Icons
demo-screen-language = Language
demo-screen-detail = Detail
demo-screen-platform = Platform
demo-screen-settings = Settings
# The rail's action: a new note, written in the side pane beside whatever screen is open.
demo-action-new = New note
# The foot slot's dot, hovered.
demo-foot-online = Online (click to go offline)
demo-foot-offline = Offline (click to go online)

demo-overview-intro = Every screen here uses corundum-framework as an app would. Make the window narrower than { $width } points to see the phone layout.
demo-overview-layout = Layout
demo-overview-rail = Rail, down the left ({ $width } pt wide)
demo-overview-bar = Bottom bar ({ $width } pt wide)
demo-overview-clicks = { $count ->
    [one] The rail's action was clicked once.
   *[other] The rail's action was clicked { $count } times.
}
demo-overview-online = The foot slot's dot says online.
demo-overview-offline = The foot slot's dot says offline.
demo-overview-open-settings = Open Settings from the app

demo-theme-settings = The theme's settings
demo-theme-picker = A TintPicker on its own
demo-theme-palette = The palette: tones by swatches
demo-theme-widgets = Widgets in this theme
demo-theme-button = Button
demo-theme-disabled = Disabled
demo-theme-checkbox = Check box
demo-theme-selectable = Selectable
demo-theme-link = A link
demo-theme-hint = A text field

demo-fonts-fallback = CJK fallback: { $family }
demo-fonts-none = no CJK font found; CJK text shows as boxes
demo-fonts-proportional = Proportional
demo-fonts-monospace = Monospace

demo-icons-intro = The { $count } bundled Lucide icons, drawn as:
demo-icons-text = Text
demo-icons-muted = Muted
demo-icons-accent = Accent
demo-icons-red = Red
demo-icons-green = Green
demo-icons-button = Button
demo-icons-selected = Selected
demo-icons-disabled = Disabled

demo-language-picker = The language picker
demo-language-current = Showing: { $language }
demo-language-available = Available: { $languages }
demo-language-system = The system's languages: { $languages }
demo-language-greeting = Hello! This sentence comes from the demo's own translations.
demo-language-clicks = { $count ->
    [one] One click so far.
   *[other] { $count } clicks so far.
}

demo-platform-system = System
demo-platform-os = { $os } on { $arch }
demo-platform-clipboard = Clipboard
demo-platform-sample = Copied from the corundum-framework demo
demo-platform-copy = Copy a sample text
demo-platform-paste = Paste
demo-platform-paste-here = Paste here
demo-platform-read = Read the system clipboard
demo-platform-clipboard-text = The system clipboard holds: { $text }
demo-platform-clipboard-none = Nothing read: here egui's own clipboard is the one to use.
demo-platform-insets = Insets
demo-platform-safe-area = Safe area (egui, points): { $insets }
demo-platform-window-insets = Window insets (Android, pixels): { $insets }
demo-platform-viewport = Viewport: { $rect }
demo-platform-content = Content: { $rect }
demo-platform-ppp = Pixels per point: { $ppp }
demo-platform-keyboard = Type here to open the keyboard
demo-platform-dialogs = Files and folders
demo-platform-no-dialogs = No file dialog on this platform yet.
demo-platform-pick-images = Pick images…
demo-platform-images = Images
demo-platform-pick-file = Pick a file…
demo-platform-pick-folder = Pick a folder…
demo-platform-show = Show in the file manager
demo-platform-media = Photos and videos
demo-platform-no-media = No photo picker here: on Android, the activity does not forward the picker's result.
demo-platform-pick-photos = Pick photos…
demo-platform-pick-video = Pick a video…
demo-platform-pick-media = Pick up to 5 photos or videos…
demo-platform-media-waiting = Waiting for the picker…
demo-platform-media-none = Nothing picked yet.
demo-platform-media-file = { $path } ({ $size } bytes)
demo-platform-qr = QR codes
demo-platform-qr-text = Text to show as a QR code
demo-platform-no-scanner = No QR code scanner here: on Android it needs Google Play services, on iOS a camera.
demo-platform-scan = Scan a QR code…
demo-platform-scanning = Waiting for the scanner…
demo-platform-scanned = Read: { $text }
demo-platform-show-scanned = Show it as a QR code
demo-platform-scanned-none = Nothing read.
demo-platform-dirs = The app's directories
demo-platform-config = Config: { $path }
demo-platform-data = Data: { $path }
demo-platform-cache = Cache: { $path }
demo-platform-none = none
demo-platform-refresh = Read again

# The Settings screen: corundum's, with the demo's pages in it.
demo-settings-demo = Demo
demo-settings-sample = Sample
demo-settings-sample-intro = A page of the app's own. A change applies with Apply; until then, its entry in the list is marked.
demo-settings-sample-name = Name
demo-settings-sample-saved = Saved: { $name }
demo-settings-apply = Apply
demo-settings-revert = Revert
demo-settings-general = General
# Where the look is kept: a file on this device (corundum's store), with the rail's order.
demo-settings-scope-device = This device
demo-settings-look-file = Saved in { $path } with the rail's order, and shown again at the next start.
demo-settings-look-memory = Kept while the demo runs: this platform gives it no config directory.
demo-settings-about = About
demo-settings-version = Version { $version }

# The Detail screen: albums, and the picked one's tracks in the side pane.
demo-detail-intro = Pick an album to see its tracks in the side pane. Wide, the pane is a right column beside every screen, which the button at the right end of the status bar hides; with it hidden, or on a phone, picking an album opens it.
demo-detail-tracks = { $count ->
    [one] { $count } track
   *[other] { $count } tracks
}

# The side pane, app-wide: a note being written, else the open screen's item, else nothing.
demo-pane-empty = Nothing to show here. Pick an album on the Detail screen, or start a note with +.
demo-note-new = New note
demo-note-save = Save
demo-note-discard = Discard

# The status bar's line: the last thing that happened.
demo-status-ready = Ready
demo-status-writing = Writing a new note
demo-status-saved = Saved the note “{ $note }”
demo-status-picked = Picked { $album }
