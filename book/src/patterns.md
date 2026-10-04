# Patterns from real extensions

Distilled from the
[Raycast extensions repository](https://github.com/raycast/extensions/tree/main/extensions)
(3,300+ published extensions; a dozen representative ones studied in
depth: Todoist, Spotify Player, Apple Reminders, Obsidian, Kill
Process, Color Picker, Emoji, Hacker News, Wikipedia, Notion, Weather,
Pomodoro). Each pattern names what the ecosystem does and how to
apply it in Corvo.

## Structure: many small commands beat one big command

Popular extensions are not one command with a menu — they are a
*family of commands*: Todoist ships 10, Spotify Player 33, Color
Picker 8, Apple Reminders 9. The mix is deliberate:

- **View commands** explore data (tasks, library, processes).
- **NoView commands with required arguments** capture without opening
  anything: `quick-add-task text`, `quick-add-reminder text`,
  `append-task text dueDate`. This "capture without UI" pattern is
  the highest-leverage command an integration can ship — one
  keystroke, type, Enter, done.
- **Niche commands ship `disabledByDefault`** — Spotify's
  `volume25`, `like`, `copyEmbed` exist but stay out of root search
  until the user opts in. Root search never saturates.

In Corvo: declare every verb as a manifest command; mark the niche
ones and the one-line captures. A NoView command with a required text
argument is the Corvo equivalent of the quick-add pattern.

## The list row is the universal component

The single biggest lesson: Raycast extensions express almost
everything as list rows with rich accessories — even the weather's
hourly forecast is one row per hour, with `icon + text + tooltip`
accessories for rain, wind, cloud cover, and humidity. Kill Process
shows CPU and memory as accessories with tooltips. Nobody builds
bespoke canvases for data that fits rows.

Consequences for Corvo extensions:

- Prefer a List page with well-designed rows over a Blocks page when
  the data is enumerable. Blocks are for a *single live datum*
  (timer, now playing), not for tables of things.
- The subtitle grammar is `X · Y · Z` (middle-dot separated facts,
  most important first).
- The section header carries the count: Raycast renders
  `<List.Section title="Processes" subtitle="245 running">`. In
  Corvo, put it in the section label: `section("Processes · 245
  running")` — the header renders it for free.
- Name one accessory per fact, right-aligned, short ("85%", "3 min
  ago"). Corvo's `SearchResult.accessory` is one string today;
  structured multi-accessories with tooltips are on the roadmap.

## The dropdown is the second filter dimension

List pages with more than one natural axis add a dropdown in the
search bar: Hacker News picks the topic (front page / Ask / Show),
Kill Process picks the sort (CPU / memory), Emoji picks the category,
Weather picks units. Text filters within the axis; the dropdown
switches the axis.

In Corvo today the axis goes in the page query or the extension adds
its own rows for each value; a first-class dropdown accessory is the
top requested kit addition (roadmap: `PageView::List { filters }`).

## Actions: grouped, hotkeyed, confirmed

- Action menus are grouped in sections: *open things* first, then
  *copy/export*, destructive last and apart.
- The common action gets a shortcut: Copy Link is ⌘. in Hacker News;
  Raycast publishes Common shortcuts (⌘S save, ⌘R refresh...) and
  asks extensions to reuse them.
- Destructive actions confirm first — Kill Process shows an alert
  before killing *and* before force-killing, and its failure alerts
  offer "Open Help" as the primary action instead of a dead error.
- After a mutation, the view goes somewhere useful on purpose: close
  the window, or pop to root with the search bar cleared — Kill
  Process makes both user preferences.

In Corvo: `actions::destructive` + the confirm convention covers
confirmation; pair every mutation with the post-action navigation the
user would want, and prefer preferences over hard-coding it.

## State: recents, preferences, and behavior toggles

- **Recently used is a first-class section**: the Emoji extension
  keeps the last 10 used emojis in persistent state and renders them
  as the first section, before the category sections. Cheap, loved,
  copied everywhere. In Corvo: `corvo_ext::storage` + a
  `section("Recently used")` row group.
- **Behavior is preferences, not settings commands**: Kill Process
  exposes `showPID`, `showPath`, `closeWindowAfterKill`,
  `clearSearchBarAfterKill`; Google Search has
  `rememberSearchHistory` and `useClipboardFallback`; Emoji has
  `primaryAction: copy | paste` — a preference that swaps the primary
  action. Small toggles shape the experience; no extension ships a
  "settings" command.
- **Primary action preference**: when two actions are equally likely
  (copy vs paste), declare a dropdown preference and honor it in the
  menu order.

## Feedback and error handling

- Failures toast with `Failure` style, a short title, and an
  actionable message; Kill Process's platform-specific kill errors
  open a help URL from the alert.
- A mutation confirms with a success toast that says what happened
  ("Timer paused"), then the list refreshes — never leave a stale row.

## Background surfaces

Menu-bar commands with refresh intervals are the second most common
shape after views (Weather 30m, Todoist 10m, Pomodoro 10s — timer in
the menu bar). Corvo's roadmap includes menu-bar surfaces; until
then, the interval pattern maps to ticking extension pages
(`Refresh::Every`).

## Operation families and delivery modes

Two shapes repeat across the most-installed utilities, both built
from NoView commands:

- **One command per operation**: Encoding Tools ships `md5`, `sha1`,
  `sha256`, `base64-encode`... and Change Case ships 22 conversions —
  each a separate root-search verb acting on the clipboard, the
  selection, or an argument. Users search the operation, not a menu
  of operations. Corvo's text-utilities consolidates; when a family
  grows past a handful of verbs, split the high-frequency ones out as
  declared NoView commands.
- **Same operation, three delivery modes**: Google Translate ships
  `instant-translate-copy`, `instant-translate-paste`, and
  `instant-translate-view` — translate the clipboard and deliver the
  result however the user wants it. Downloads Manager ships
  open/copy/paste/show/delete-*latest-download*. Model delivery as
  commands, not as options inside one command.
- **A command that toggles a preference**: Downloads Manager's
  `toggle-deletion-behavior` flips a setting from the keyboard
  without opening Settings. Small, surprising, loved.

## Health and onboarding as a first-class page

Notion ships `manage-connection`: a Detail page that reports the
connection state in plain language ("Your connection is working."),
with actions to Reconnect, Test Connection, and Open Preferences.
Any extension with a token or an external dependency deserves the
same page: state, explanation, and the fixing actions on one screen.

## Computation in the search bar

Currency Exchange turns the search text into the input — type
`100 usd` and every row is a converted amount, updated per keystroke,
with the currency name as an accessory. The search bar is an input
field, not just a filter: commands that transform (converters,
calculators, encoders) should render their output as rows driven by
the query.

## Grids: images with sections and columns

Unsplash renders results as a Grid with `Grid.Section` (title +
count), column count from a preference, and images as tile content.
In Corvo, Grids filter from the search bar and take a column count
per page; image tiles take a cached PNG path.

## Adopted in the kit

The patterns above map to first-class kit support: filter chips and
structured accessories (`corvo_ext::pages`), the Recently used
helper (`corvo_ext::recents` — the emoji picker uses it for its
strip), per-row pushed details (`Action::ShowDetail`), grid sections
(`GridItem.section`), declared NoView capture commands (the template
ships one), and clipboard quick verbs (text-utilities declares
To UPPERCASE, to snake_case, Base64 Encode, SHA-256... as NoView
commands).

## Anti-patterns to avoid

- One mega command with an internal menu of sub-features.
- Bespoke visual canvases for enumerable data.
- Settings implemented as commands instead of preferences.
- Destructive actions without confirmation.
- Stale rows after a mutation — always refresh the page.
- Empty states that say nothing; say what would appear and how.
