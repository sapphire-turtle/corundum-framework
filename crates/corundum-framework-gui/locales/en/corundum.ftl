# corundum-framework-gui's own strings. Keys are `corundum-<area>-<what>`.

# This language's name, in itself: how the language picker lists it.
corundum-language-name = English
corundum-language = Language
# The picker's System entry, with the language it resolves to now.
corundum-language-system = System ({ $language })

# The theme settings (`theme::settings_ui`).
corundum-theme-appearance = Appearance
corundum-theme-light = Light
corundum-theme-dark = Dark
corundum-theme-system = System
corundum-theme-contrast = Contrast
corundum-theme-low = Low
corundum-theme-medium = Medium
corundum-theme-high = High
# The accent row's label.
corundum-theme-accent = Accent

# The accent picker (`theme::TintPicker`): the chips' hover names, and the preview's words.
corundum-theme-default = Default
corundum-theme-random = Random
corundum-theme-swatch-neutral = Neutral
corundum-theme-swatch-red = Red
corundum-theme-swatch-orange = Orange
corundum-theme-swatch-yellow = Yellow
corundum-theme-swatch-green = Green
corundum-theme-swatch-cyan = Cyan
corundum-theme-swatch-blue = Blue
corundum-theme-swatch-purple = Purple
corundum-theme-link = Link
corundum-theme-label = Label
corundum-theme-selected = Selected

# The Settings screen (`settings`): the way back to its list on a phone, and the Appearance
# page's heading.
corundum-settings = Settings
corundum-settings-appearance = Appearance

# The detail pane (`detail`): the way back on a phone, the column's toggle, the card's close.
corundum-detail-back = Back
corundum-detail-show = Show the detail pane
corundum-detail-hide = Hide the detail pane
corundum-detail-close = Close

# The shell's More button (`Shell`), in the footer's place when items are left off the rail.
corundum-shell-more = More

# The rail's settings page (`settings::RailPage`): its name, what it does, a row's handle and
# box, and the way back to the app's order.
corundum-settings-rail = Navigation
corundum-settings-rail-intro = Drag the items into the order you use them. Unticked items are in the ⋯ menu, with those the window has no room for.
corundum-settings-rail-drag = Drag to reorder
corundum-settings-rail-show = Show as a button
corundum-settings-rail-reset = Reset to the default order

# The update panel (`update::UpdatesPanel`): the button that looks again, a row's running
# version when unknown, and each row's state. A release this app does not install says how
# it is had instead: the command to run (with a Copy button), or who updates it.
corundum-update-check = Check for updates
corundum-update-not-running = Not running
corundum-update-checking = Checking…
corundum-update-up-to-date = Up to date
corundum-update-newer = { $version } is available
corundum-update-update = Update
corundum-update-release-notes = Release notes
corundum-update-downloading = Downloading { $version }…
corundum-update-restarting = Restarting…
corundum-update-done = Updated to { $version }
corundum-update-restart-by-hand = Updated; restart it to use { $version }
corundum-update-failed = Failed: { $error }
# The error of a row whose background work stopped unexpectedly.
corundum-update-worker-stopped = the update stopped unexpectedly
corundum-update-retry = Retry
corundum-update-run = Run:
corundum-update-copy = Copy
corundum-update-by-cargo = Update it with { $tool }
corundum-update-by-package-manager = Updated by your package manager
corundum-update-by-the-app = Updated by the app itself: Settings › About
corundum-update-by-flathub = Updated by Flathub
corundum-update-by-snap = Updated by the Snap Store
corundum-update-by-microsoft-store = Updated by the Microsoft Store
corundum-update-by-mac-app-store = Updated by the Mac App Store
corundum-update-from-release-page = Download it from the release page

# The app-menu row (`settings::LauncherRow`): its label, its two buttons, and why a click did
# nothing.
corundum-launcher-in-menu = In the app menu
corundum-launcher-remove = Remove
corundum-launcher-add = Add back
corundum-launcher-failed = Could not change the app menu: { $error }
