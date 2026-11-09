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
corundum-update-by-the-app = Updated by the app itself
corundum-update-by-flathub = Updated by Flathub
corundum-update-by-snap = Updated by the Snap Store
corundum-update-by-microsoft-store = Updated by the Microsoft Store
corundum-update-by-mac-app-store = Updated by the Mac App Store
corundum-update-from-release-page = Download it from the release page
# A package's row (`update::Target::Package`, `Target::Delegated`): the package updates
# itself, and a downloaded update is applied by restarting. Its failure and Retry are the
# rows' above.
corundum-update-package-current = { $version }
corundum-update-package-uptodate = Up to date ({ $version })
corundum-update-package-downloading = Downloading…
corundum-update-package-ready = { $version } is ready
corundum-update-package-restart = Restart to update

# The app-menu row (`settings::LauncherRow`): its label, its two buttons, and why a click did
# nothing.
corundum-launcher-in-menu = In the app menu
corundum-launcher-remove = Remove
corundum-launcher-add = Add back
corundum-launcher-failed = Could not change the app menu: { $error }

# The status bar's update icon (`update::ReadyBadge`): its tooltip.
corundum-update-badge = Restart to update to { $version }

# The About page's command line row (`settings::CliRow`): its label, its buttons, what it says
# when it cannot offer one, and why a click did nothing.
corundum-cli-row = Command line: { $command }
corundum-cli-install = Install
corundum-cli-remove = Remove
corundum-cli-repair = Repair
corundum-cli-taken = Another { $command } is installed there; remove it first
corundum-cli-not-on-path = ~/.local/bin is not on PATH
corundum-cli-status-installed = On PATH
corundum-cli-status-missing = Not on PATH
corundum-cli-failed = Could not change the command line: { $error }

# The file explorer (`explorer::FileExplorer`): its buttons, the list's headings, what shows
# with no entries, the entry's menu, and a drag of several entries.
corundum-explorer-up = Up to the folder above
corundum-explorer-refresh = Refresh
corundum-explorer-list = List
corundum-explorer-grid = Grid
corundum-explorer-name = Name
corundum-explorer-size = Size
corundum-explorer-modified = Modified
corundum-explorer-loading = Loading…
corundum-explorer-empty = This folder is empty
corundum-explorer-failed = Could not list this folder: { $error }
corundum-explorer-retry = Retry
corundum-explorer-open = Open
corundum-explorer-rename = Rename
corundum-explorer-items =
    { $count ->
        [one] { $count } item
       *[other] { $count } items
    }

# The desktop's photo and video dialog (`platform::MediaPicker`): its filters.
corundum-media-images-and-videos = Images and videos
corundum-media-images = Images
corundum-media-videos = Videos

# A QR code (`qr::QrCode`) whose text is too long for one.
corundum-qr-too-long = Too long to show as a QR code.

# The first-run flow (`first_run::FirstRun`): the step in the header, and the footer buttons.
corundum-first-run-step = Step { $step } of { $count }
corundum-first-run-back = Back
corundum-first-run-next = Next
corundum-first-run-finish = Finish

# The first-run page asking to keep the app's services running in the background
# (`first_run::ServicePage`): its answers, a development build's note, and a failed install.
corundum-service-yes = Yes, keep it running
corundum-service-no = Not now
corundum-service-dev-build = This is a development build; nothing is registered.
corundum-service-failed = Could not register: { $error }
corundum-service-try-again = Try again
corundum-service-continue-without = Continue without it

# The About page's row for a background service (`settings::ServiceRow`): its label, the
# service's state, and its buttons.
corundum-service-row = Background: { $name }
corundum-service-run = Keep running in the background
corundum-service-running = Running
corundum-service-stopped = Stopped
corundum-service-stop = Stop
corundum-service-start = Start
corundum-service-remove = Remove
corundum-service-broken = Not working: { $reason }
corundum-service-repair = Repair

# The API keys page (`keys::KeysPage`): a client's key, its token shown once when made.
corundum-keys-title = API keys
corundum-keys-add = Add
corundum-keys-name = Name
corundum-keys-description = Description
corundum-keys-retire = Retire
corundum-keys-restore = Restore
corundum-keys-rotate = Rotate
corundum-keys-retired = Retired
corundum-keys-confirm-retire = Retire { $name }? Clients using it stop working.
corundum-keys-confirm-rotate = Issue a new token for { $name }? The old one stops working.
corundum-keys-confirm = OK
corundum-keys-cancel = Cancel
corundum-keys-token-once = Copy this token now. It is shown only once.
corundum-keys-copy = Copy
corundum-keys-close = Close
corundum-keys-none = No keys yet.
corundum-keys-show-retired = Show retired
corundum-keys-profile = Profile
corundum-keys-profile-choose = Choose a profile
corundum-keys-profile-none = There are no profiles to choose from.
corundum-keys-profile-unknown = { $id } (unknown)

# The image preview (`preview::ImagePreview`): why a file shows no picture.
corundum-preview-unsupported = This file can't be previewed.
corundum-preview-too-large = This image is too large to preview.
corundum-preview-failed = Couldn't read this file: { $reason }
corundum-preview-retry = Retry

# The PDF preview (`preview::PdfPreview`): the page buttons and why a PDF shows no page.
corundum-preview-pdf-page = { $page } / { $count }
corundum-preview-pdf-page-of = { $page } / { $shown } of { $count }
corundum-preview-pdf-previous = Previous page
corundum-preview-pdf-next = Next page
corundum-preview-pdf-encrypted = This PDF is password-protected and can't be previewed.
corundum-preview-pdf-broken = Couldn't read this PDF: { $reason }
corundum-preview-pdf-too-large = This PDF is too large to preview.

# The audio preview (`preview::AudioPreview`): its button, and why the device did not play.
# A file that is not audio, or that failed, says so with the strings above.
corundum-preview-play = Play
corundum-preview-pause = Pause
corundum-preview-audio-device = Couldn't play through the audio device: { $reason }

# The info card (`preview::InfoCard`): its rows, the kind of a file, and the open button.
corundum-preview-size = Size
corundum-preview-kind = Kind
corundum-preview-modified = Modified
corundum-preview-created = Created
corundum-preview-open = Open with the system's app
corundum-preview-kind-image = { $ext } image
corundum-preview-kind-audio = { $ext } audio
corundum-preview-kind-video = { $ext } video
corundum-preview-kind-text = { $ext } document
corundum-preview-kind-other = { $ext } file
corundum-preview-kind-unknown = File

# The text preview (`preview::TextPreview`): the note on a file shown only in part.
corundum-preview-text-cut = Showing the first { $size }.

# The Markdown toolbar (`markdown::toolbar`): a button's label, shown when it has no icon, and
# its name, for screen readers and the hover text. Labels stay ASCII.
corundum-markdown-strong-label = B
corundum-markdown-emphasis-label = I
corundum-markdown-strikethrough-label = S
corundum-markdown-code-label = </>
corundum-markdown-link-label = Link
corundum-markdown-heading-label = H
corundum-markdown-bullet-list-label = -
corundum-markdown-numbered-list-label = 1.
corundum-markdown-task-list-label = [ ]
corundum-markdown-quote-label = >
corundum-markdown-code-block-label = { "{ }" }
corundum-markdown-strong-name = Bold
corundum-markdown-emphasis-name = Italic
corundum-markdown-strikethrough-name = Strikethrough
corundum-markdown-code-name = Code
corundum-markdown-link-name = Link
corundum-markdown-heading-name = Heading
corundum-markdown-bullet-list-name = Bulleted list
corundum-markdown-numbered-list-name = Numbered list
corundum-markdown-task-list-name = Task list
corundum-markdown-quote-name = Quote
corundum-markdown-code-block-name = Code block

# alumina-datetime's pickers (the `datetime` feature)
corundum-datetime-month-1 = January
corundum-datetime-month-2 = February
corundum-datetime-month-3 = March
corundum-datetime-month-4 = April
corundum-datetime-month-5 = May
corundum-datetime-month-6 = June
corundum-datetime-month-7 = July
corundum-datetime-month-8 = August
corundum-datetime-month-9 = September
corundum-datetime-month-10 = October
corundum-datetime-month-11 = November
corundum-datetime-month-12 = December
corundum-datetime-month-short-1 = Jan
corundum-datetime-month-short-2 = Feb
corundum-datetime-month-short-3 = Mar
corundum-datetime-month-short-4 = Apr
corundum-datetime-month-short-5 = May
corundum-datetime-month-short-6 = Jun
corundum-datetime-month-short-7 = Jul
corundum-datetime-month-short-8 = Aug
corundum-datetime-month-short-9 = Sep
corundum-datetime-month-short-10 = Oct
corundum-datetime-month-short-11 = Nov
corundum-datetime-month-short-12 = Dec
corundum-datetime-weekday-mon = Mo
corundum-datetime-weekday-tue = Tu
corundum-datetime-weekday-wed = We
corundum-datetime-weekday-thu = Th
corundum-datetime-weekday-fri = Fr
corundum-datetime-weekday-sat = Sa
corundum-datetime-weekday-sun = Su
corundum-datetime-week-column = W
corundum-datetime-day = Day
corundum-datetime-week = Week
corundum-datetime-month = Month
corundum-datetime-year = Year
corundum-datetime-range = Range
corundum-datetime-today = Today
corundum-datetime-now = Now
corundum-datetime-clear = Clear
corundum-datetime-previous = Previous
corundum-datetime-next = Next
corundum-datetime-choose-month = Choose a month
corundum-datetime-choose-year = Choose a year
corundum-datetime-am = AM
corundum-datetime-pm = PM
corundum-datetime-none = —
corundum-datetime-period-hint = 2026-11-03, 2026-W45, 2026-11, 2026 …

# A workspace driven over fs-client (`fs_client::ExplorerDriver`): the entry's menu item, the
# labels of the changes it applies, the notice after one, and a server of another version.
corundum-fs-trash = Move to the trash
corundum-fs-label-rename = Rename “{ $name }”
corundum-fs-label-move =
    { $count ->
        [one] Move { $count } item
       *[other] Move { $count } items
    }
corundum-fs-label-trash =
    { $count ->
        [one] Move { $count } item to the trash
       *[other] Move { $count } items to the trash
    }
corundum-fs-done = { $label }
corundum-fs-skipped = {" "}({ $count } skipped)
corundum-fs-close = Close
corundum-fs-api-mismatch = This app's file API (version { $client }) does not match the server's (version { $server })
