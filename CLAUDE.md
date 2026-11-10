# Grades App

Modern minimalist desktop app that shows grades per class, per term, and cumulative GPA (standard 4.0 scale). Features: target-grade solver, what-if scenarios, grade analysis (impact, leverage, trends). Every capability is available to LLM agents via MCP and CLI (primary agent: Hermes Agent on the user's home server). Cross-device sync. Platforms: Windows + macOS desktop, iOS later. Language: Rust.

## Workspace layout

```
core/    domain model, grade math, all actions as typed Commands (pure, no IO)
store/   SQLite (rusqlite, "bundled") + operation log for sync
app/     Tauri 2 app (Windows, macOS, later iOS)
mcp/     MCP tools via the `rmcp` crate, 1:1 with core Commands
cli/     `grades` CLI (clap); every subcommand supports --json
server/  Axum sync server + MCP over HTTP (home server)
```

Only create crates when their milestone starts. Do NOT scaffold `app/`, `mcp/`, or `server/` yet.

## PARITY RULE (non-negotiable)

**No feature exists in any UI unless it is a core `Command` first.** UI, CLI, and MCP are thin adapters over core Commands. Never put grade math or business logic in an adapter. If a person can do it, an LLM must be able to do it through the same Command.

## Current milestone: 1 — core + cli

Create now: `core` (full), `store` (optional, minimal: SQLite persistence only, no sync yet), `cli`.

Done means every item below is implemented, unit-tested, and exposed as a Command + `grades` subcommand with `--json`. Tick items here as you finish them.

- [ ] Data model: Term -> Course (credits, letter cutoffs, curve settings, target) -> Category (weight, drop-lowest N) -> Assignment (points possible, score, due date, status, optional curve); serde derives
- [ ] Current grade (renormalized over categories with graded work) and projected grade (full weights)
- [ ] Drop-lowest N within categories
- [ ] Target solver with locked expected scores; results Secured / OnTrack{needed} / Unreachable
- [ ] Per-assignment curves (add N points; scale top score to 100%; set new max)
- [ ] Per-course curves (add N points to final %; shift letter cutoffs); solver reports without curve by default, optionally with expected curve
- [ ] What-if: named scenarios overlay hypothetical scores/curves, never mutate real data, comparable side by side (grade and GPA)
- [ ] Analysis: impact (running-grade change per assignment, chronological), leverage (final-grade change per 10% scored on each remaining assignment), trends over time
- [ ] GPA: per-term and cumulative, editable scale, exclusions
- [ ] CLI covering all of the above; `--json` everywhere
- [ ] Required edge-case tests (below)

Required edge-case tests:
- No graded work at all; all categories empty
- Some categories empty (weights renormalize)
- Drop-lowest N with fewer than (or exactly) N assignments
- Target already secured; target unreachable (needed > 100%); target exactly achievable
- Locked expected scores in solver
- Each curve type, alone and combined; curves do not mutate stored scores
- What-if scenario leaves real data untouched
- Pass/fail and withdrawn courses excluded from GPA; retaken courses excluded; zero-credit courses
- Cumulative GPA with no eligible courses (no divide-by-zero)
- Floating-point: compare with epsilon

## Domain rules

**Current grade** = weighted average of category averages, renormalized over categories that have graded work. **Projected grade** uses full weights.

**Target solver:** binary-search the single uniform score on remaining ungraded work that hits the target. Valid because grade is monotonic in scores, even with drops and curves. Support locking expected scores on specific assignments. Result states: `Secured`, `OnTrack{needed}`, `Unreachable{needed > 100%}`.

**Curves are assumptions, not data.**
- Per-assignment: add N points; scale so top score = 100%; set new max.
- Per-course: add N points to final %; shift letter cutoffs.
- Solver reports the requirement without any curve by default; optionally with the expected curve.

**What-if:** named scenarios overlay hypothetical scores/curves on real data. Never mutate real data. Scenarios are comparable side by side, including GPA.

**Analysis:** impact = change in running grade when each assignment was graded (chronological). Leverage = final-grade change per 10% scored on each remaining assignment. Trends = grade over time.

**GPA (editable scale):** A/A+ 4.0, A- 3.7, B+ 3.3, B 3.0, B- 2.7, C+ 2.3, C 2.0, C- 1.7, D+ 1.3, D 1.0, F 0.
Cumulative = sum(points x credits) / sum(credits). Exclude pass/fail, withdrawals, and retaken courses.

## Conventions

- Rust stable, edition 2024.
- `thiserror` for library errors; `anyhow` only in binaries (`cli`, later `server`/`app`).
- `serde` derives on all domain types and Command inputs/outputs (needed for CLI/MCP JSON).
- `core` has no IO and no platform-specific code. No clock, filesystem, or randomness: pass them in.
- Percentages are `f64` in 0.0-100.0 everywhere (not 0-1). Compare with an epsilon in tests.
- Commands are an enum (or set of structs) that return typed results; adapters only parse input and render output.
- Prefer small, pure functions with direct unit tests. No `unwrap()` in non-test code.

## Cross-platform

Developed on Windows now, macOS (and iOS builds) later.
- No hard-coded paths. Use the `dirs` crate for data/config locations.
- No shell-specific scripts (bash/PowerShell). Put automation in `cargo` commands or Rust.
- `rusqlite` with the `bundled` feature so no system SQLite is needed.
- Use `std::path::Path`/`PathBuf`; never build paths with string concatenation or `/` or `\`.
- Avoid `#[cfg(windows)]`/`#[cfg(unix)]` outside adapter crates.

## Before finishing any task

Run all three; all must pass cleanly:

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Workflow

- Make small commits with clear messages.
- Update the milestone checklist in this file as items are completed.
- Ask before adding heavy dependencies.
- Never commit secrets, tokens, or local databases.

## Later milestones (do not build yet)

2. Local MCP (stdio). 3. Desktop app (Tauri 2, Windows + macOS). 4. Sync server + remote MCP on home server (Tailscale). Sync design: local SQLite per device, every Command appended to an op log (timestamp + device id), push/pull to server, last-write-wins per field. 5. iOS app; optional Canvas LMS import.
