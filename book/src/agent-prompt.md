# Prompt for coding agents

Contributors increasingly let a coding agent write the first
version. The prompt below is the standard input: it compresses the
contribution standard into a paste-ready block. The same content
ships as [`PROMPT.md`](https://github.com/diegoleteliers10/corvo/blob/main/PROMPT.md)
at the repo root, and `AGENT.md` carries the equivalent rules for
agents that read the repository automatically.

{{#include ../../PROMPT.md:2:}}

## Tips for agent-authored extensions

- Review the diff like any PR — the agent has no taste, the
  checklist does. The [contribution standard](contributing.md)
  applies unchanged.
- Ask the agent for the manual-verification list: CI cannot grant
  Accessibility permission or launch Spotify. A summary that claims
  everything "works" without naming what was actually run is a red
  flag.
- Small, focused prompts beat mega-prompts: one extension, or one
  improvement to one extension, per session.
