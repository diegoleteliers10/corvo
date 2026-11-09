# Corvo 0.7.2

The version Corvo reports is now the real one, and calculator history has its own page.

## Updates

- About and the update check now read the release version from the app manifest instead of a stale crate version. Before this, a freshly updated app still displayed 0.6.0 and kept offering the same update.

## Calculator

- The calculator history opens as its own page, like clipboard history. It lists every stored evaluation with how long ago it was made, filters as you type, and lets you copy a result with Enter or paste it into the previous app with Cmd+Enter.
- The built-in Calculator History command opens that page directly.
- An empty page says there are no calculations yet, and clearing history refreshes the page.

# Corvo 0.7.1

Calculator fixes that landed after 0.7.0.

## Calculator

- A bare expression such as `10/2` now answers while typing, without the `calculator` keyword.
- Ordinary searches no longer return calculator history rows. History appears only in the scoped `calculator` search.
- History saves an evaluation when the user commits it, so typing `123` no longer stores `1`, `12` and `123`.
- The shared history tests are serialized, so they stop racing under parallel test execution.
