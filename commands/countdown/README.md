# Countdown

Days until a date, with a live page and saved countdowns. The
reference implementation of Corvo's declarative page model: **zero
corvo-ui code** — the manifest declares the command surface,
`Command::page` returns Blocks, and the launcher renders everything.

## Usage

- `countdown` — open the page, then type a date.
- `countdown 2026-12-25` — countdown to an ISO date.
- `countdown Dec 25 2026 My trip` — month-name dates with a label.
- `countdown 25 December` — this or next year, whichever has it.
- Enter on a running countdown saves it to the strip; Remove buttons
  take saved entries back off.

## Model notes

- Saved countdowns live in the extension's key-value storage
  (`corvo_ext::storage`), one `cd-<target>-<label>` key each.
- The page ticks at `Refresh::Every(1)`; under a second of remaining
  time the hero shows the clock instead of whole days.
- Past dates flip the badge to `PAST` and count backwards.
