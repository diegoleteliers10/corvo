# Corvo 0.7.1

Calculator fixes that landed after 0.7.0.

## Calculator

- A bare expression such as `10/2` now answers while typing, without the `calculator` keyword.
- Ordinary searches no longer return calculator history rows. History appears only in the scoped `calculator` search.
- History saves an evaluation when the user commits it, so typing `123` no longer stores `1`, `12` and `123`.
- The shared history tests are serialized, so they stop racing under parallel test execution.
