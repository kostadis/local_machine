# Implementation Plan: `local-machine hypostasis` — Bring Up the DMsGuild Library Palace

**Branch**: `003-dmsguild-library-palace` | **Date**: 2026-11-20 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/003-dmsguild-library-palace/spec.md`

## Summary

Add the DMsGuild library palace as one more data piece of `local-machine hypostasis`.
`setup` has CampaignGenerator's `library build` build it when it is missing, after checking
keys, binary, DB, source folder, live-palace guard and embedder. `status` and `start` read
its state from `library status --json` and map the exit codes (3 → 3, 4 → 4, others → 1).
Every `library` call gets the authority's `env:` block set on the child process, so the
owner's shell can't choose the embedder. All library facts are read from hypostasis's
authority file; local-machine config gains nothing.

## Technical Context

**Language/Version**: Rust (edition and toolchain as in `Cargo.toml`, unchanged)

**Primary Dependencies**: existing only (`anyhow`, `serde_yaml_ng`, `serde_json` if already
present, else add `serde_json` to parse `library status --json`; see tasks)

**Storage**: none of our own; the palace is CampaignGenerator's, at `library.palace`

**Testing**: `cargo test` with `FakeSystem` (`src/system/fake.rs`); manual run per
`quickstart.md`

**Target Platform**: Ubuntu (the owner's VM), as spec 002

**Project Type**: CLI (single Rust crate)

**Performance Goals**: no added latency to `status`/`start` beyond one `library status`
call and one 3 s-bounded HTTP probe. `library build` has no timeout (R8).

**Constraints**: no root; no prompt; never overwrite or delete an existing library;
remove only a palace path created by this run (R6)

**Scale/Scope**: one new module (`src/hypostasis/library.rs`, ~300 lines with tests), small
additions to `authority.rs`, `setup.rs`, `status.rs`, `start.rs`, `system/mod.rs`,
`system/fake.rs`

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Check | Result |
|---|---|---|
| I. Single entry point | No new subcommand or action; extends `hypostasis setup/start/status`. Spec names subcommand and actions. | Pass |
| II. Configuration | No machine literals: every path and URL comes from the authority. `~/src/campaigns` in the guard comes from 044's contract and is checked in addition to `data_roots.campaigns` (R7). Config-sharing review recorded (research R12): no new values. Authority read, never written (R11). | Pass, see note |
| III. Shared parameters | No new parameters (R13). | Pass |
| IV. Rust | All logic in Rust; only `library` (CampaignGenerator) is invoked as an external program. | Pass |
| V. Ubuntu | Nothing distribution-specific added. | Pass |
| VI. Privileged actions | No library step needs root; `notice()` returns `None`; status/start never prompt; tests assert `FakeSystem::prompted()` is false on every library path. | Pass |

Note on II: `~/src/campaigns` is a guard path from the downstream contract, not a fact
about this machine; it is built from the owner's home, not a literal path. If the owner
prefers, it can be dropped and only the authority's `data_roots` used. No violation.

**Post-design re-check (after Phase 1)**: unchanged; all pass. No Complexity Tracking
entries.

## Project Structure

### Documentation (this feature)

```text
specs/003-dmsguild-library-palace/
├── spec.md
├── plan.md              # this file
├── research.md          # Phase 0
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/cli.md     # Phase 1
├── checklists/requirements.md
└── tasks.md             # /speckit-tasks (not yet)
```

### Source Code (repository root)

```text
src/
├── system/
│   ├── mod.rs           # Cmd.env, http_status_at, canonical (R2, R3, R7)
│   └── fake.rs          # FakeSystem support for the three additions
└── hypostasis/
    ├── authority.rs     # env, library keys, data_roots.mempalace (data-model)
    ├── library.rs       # NEW: observe(), preconditions, guard, LibraryPalace Step, exit map
    ├── mod.rs           # pub mod library
    ├── setup.rs         # LibraryPalace appended to restore_steps (R10)
    ├── status.rs        # library line + exit precedence (R9)
    └── start.rs         # library availability after services (R9)
tests/
└── fixtures/
    ├── authority.yaml               # gains env + library block
    └── authority-no-library.yaml    # NEW: not-configured case
```

Outside this repo (dependency, own branch): `~/src/hypostasis-config/hypostasis.yaml` gains
the `library:` block (R11).

**Structure Decision**: single crate, one new module beside the existing restore pieces.

## Phase 0: Research

`research.md`, R1–R13. No open NEEDS CLARIFICATION.

## Phase 1: Design

- `data-model.md`: authority additions, `LibraryState`, preconditions P1–P6, lifecycle,
  `System` additions.
- `contracts/cli.md`: output lines and exit precedence for setup/status/start/stop.
- `quickstart.md`: automated scenarios and the manual run on the machine.

## Execution Model

- **Opus orchestrates.** The main session (Opus) does not write feature code. It hands
  each task, or group of tasks for one file, to a **Sonnet** subagent (Agent tool,
  `model: "sonnet"`) with a self-contained brief: the task text, the files to read
  (spec, data-model, contracts/cli.md, the code it touches) and the done condition
  (`cargo test` + `cargo clippy -- -D warnings` green). Opus checks each result against
  the task and the contract, marks the task `[X]`, and commits on the feature branch.
- **Sonnet implements.** One subagent per task group, run in order of the dependencies in
  tasks.md. Parallel `[P]` groups that touch different files may run at the same time.
- **One review pass.** When all tasks are done, run `/code-review` once on the branch
  diff. Opus fixes the confirmed findings (again by Sonnet subagent) in that one pass.
- **After that, issues only.** Anything found later (further review findings, nits,
  follow-ups, drift against 044) is filed as a GitHub issue on `kostadis/local_machine`,
  not fixed on this branch. Then the branch is merged.

## Risks

- **CampaignGenerator 044 not merged.** The contract files are uncommitted in
  CampaignGenerator's working tree. Implementation can proceed against `FakeSystem`; the
  manual quickstart waits for 044. Re-check the contracts when they are committed.
- **Deleting on failure** (R6) is the one destructive path. It is limited to a path that
  was missing before this run and passed the guard; tests must cover a refusal to delete in
  every other case.

## Complexity Tracking

None.
