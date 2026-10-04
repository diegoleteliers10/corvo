# Corvo 0.5.0

## Extension platform

- Extensions become first-class: every command declares a manifest — title, description, icon, and a command surface with typed arguments (text, password, dropdown) — that root search and Settings render without hand-written plumbing.
- New declarative page model: a command returns a PageView and the launcher renders it. Compose Blocks (badge, hero, progress, markdown, strip cards, buttons) with a live refresh cadence, a markdown Detail with structured metadata, a Grid of tiles with sections, or a Form with text, checkbox, and select fields. Per-block Style puts colors, sizes, and backgrounds in the extension's hands.
- Declarative pages own their keyboard: left, right, and Enter mean what the command decides and the command renders the selection itself. The pomodoro picks durations with the arrows and starts with Enter; media moves between tracks.
- Filter chips give pages a second filter axis; clicking one re-renders the page with the command's own selection. Weather ships a live °C/°F toggle as the demonstration.
- Rows carry structured accessories with hover tooltips, and a recents helper backs the "Recently used" section the emoji picker now shows.
- A row can push a Detail view onto the launcher's navigation stack (title, markdown, metadata); Escape and backspace pop back.

## Extensions

- One extension per browser: Google Chrome, Brave, Microsoft Edge, Firefox, Arc, Dia, Safari, and Aside each get tabs, bookmarks, and their real application icon, appearing only while the browser is installed. Live tabs on macOS go through AppleScript; on Windows and Linux through the browser's DevTools endpoint when it runs with its debugging port.
- Media Control works on all three systems: Spotify and Music on macOS, System Media Transport Controls on Windows, MPRIS on Linux — browser playback included through those sessions.
- New Countdown extension: a live ticking page with a typed date argument (ISO, month names, or day-month) and saved countdowns, built entirely on the declarative model.
- New one-per-operation quick verbs in Text Utilities: To UPPERCASE, to lowercase, camelCase, snake_case, kebab-case, Base64 Encode and Decode, and SHA-256 act on the clipboard or the typed argument and copy the result.
- The new extension book (https://diegoleteliers10.github.io/corvo/) documents the model, the kit modules, and the patterns distilled from three thousand published launcher extensions.

## Launcher

- Extension pages render inside the standard window and scroll when their content exceeds it.
- The actions panel, toasts, and row accessories keep working for every command on all three systems.

## Platform requirements

- Browser live tabs on Windows and Linux require the browser to run with its debugging port; each page shows the exact flag.
- macOS browser and media commands require Automation permission for the target application. Safari bookmarks may need Full Disk Access.
- Linux media control requires an MPRIS player on the session bus; desktop notifications require notify-send and a notification service.
- Weather requires network access. Notes, countdowns, and timer state stay local.
- CI checks compilation and tests on Windows x64, macOS Intel, and Linux x64. Native UI, media sessions, and notification delivery still need manual checks on each system.
