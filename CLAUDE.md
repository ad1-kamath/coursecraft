# CLAUDE.md

@AGENTS.md

## Working agreements for Claude Code

- Before finishing any change, run `cargo xtask ci` (fmt, clippy -D warnings, tests, layers) and make it pass.
- Runtime behaviour changes update `docs/spec/triggers.md` in the same commit.
- Current focus is in `docs/roadmap.md`, under the first milestone not marked ✅.
- Commit per working step with a clear message. Never commit `target/`.
