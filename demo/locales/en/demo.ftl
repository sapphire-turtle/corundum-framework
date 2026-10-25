# The demo's own strings. Keys are `demo-<screen>-<what>`; corundum-framework-gui's own
# (the theme and language settings) come from its bundle.

demo-title = corundum-framework demo

# The screens, as the rail's hover text names them.
demo-screen-overview = Overview
demo-screen-theme = Theme
demo-screen-fonts = Fonts
demo-screen-icons = Icons
demo-screen-language = Language
demo-screen-platform = Platform
demo-screen-settings = Settings
# The rail's action: counts its clicks on the Overview.
demo-action-count = Count a click
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
# The scopes a look is kept in: the server's for every device, and this device's own.
demo-settings-scope-server = Server
demo-settings-scope-device = This device
demo-settings-from-server = This device uses the server's settings.
demo-settings-own = This device uses its own settings.
demo-settings-use-server = Use the server's
demo-settings-device-wins = This device's own settings win over these.
demo-settings-about = About
demo-settings-version = Version { $version }
