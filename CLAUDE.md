# AmpCraft: working notes for Claude

Read [AGENTS.md](AGENTS.md) first: its rules (asset policy, clean room, never crash) bind you
and win over anything here.

@AGENTS.md
@AGENTS.local.md

## Start every session

1. Read `ROADMAP.md` and the top of `docs/gaps.md`; maintainers also read `plan/OVERNIGHT.md`.
2. Run `scripts/check.sh` before you change anything, so you know the tree is green.

## Habits

- Build and test only through Podman (`scripts/dev.sh`, `scripts/check.sh`).
- Look at UI changes: `cargo run -p ampcraft --example theme_gallery -- OUT_DIR 2`, or a GUI
  check script's screenshots, then read the image.
- Keep `ROADMAP.md` and `docs/gaps.md` current.

## Memory

Memories for this project live in `.claude/memory/` (gitignored), one fact per file with an
index in `.claude/memory/MEMORY.md`. Write new memories there, not in `~/.claude/projects/`.

@.claude/memory/MEMORY.md
