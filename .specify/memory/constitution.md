<!--
Sync Impact Report
- Version change: 1.1.0 → 1.2.0 (MINOR: materially expanded Principle I)
- Modified principles:
  - I. Single Entry Point: adds command grammar `local-machine <subcommand> <action>`,
    shared action verbs, and the requirement that specs name their subcommand and actions
- Added sections: none
- Removed sections: none
- CLAUDE.md updated in the same change (Governance sync rule)

Previous report (1.1.0):
- Version change: 1.0.0 → 1.1.0 (MINOR: materially expanded Principle II)
- Modified principles:
  - II. Configuration, Not Hardcoding (YAML): adds config storage location
    (`~/.config/local-machine` default, overridable via shared `--config-dir` parameter)
- Added sections: none
- Removed sections: none
- CLAUDE.md updated in the same change (Governance sync rule)

Previous report (1.0.0):
- Version change: (unversioned template) → 1.0.0
- Source: derived from /home/kostadis/src/local_machine/CLAUDE.md
- Principles (all new):
  - I. Single Entry Point: `local-machine`
  - II. Configuration, Not Hardcoding (YAML)
  - III. Shared Parameter Vocabulary
  - IV. Rust Implementation
  - V. Ubuntu Linux Target
- Added sections: Scope & Machine Boundary; Source Control; Governance
- Removed sections: none
- Templates: none modified (dependent templates read the constitution at runtime)
- Deferred TODOs: none. Note: "There will be a local repo" in CLAUDE.md is interpreted as
  "the tool's source lives in a local git repository"; no remote is assumed.
-->

# local-machine Constitution

## Core Principles

### I. Single Entry Point: `local-machine`

All scripts and tools in this project MUST be reachable as subcommands of one top-level
command named `local-machine`. No standalone binaries or loose scripts may be added as a
separate user-facing entry point. Every subcommand MUST be discoverable from
`local-machine --help` (or the help of its parent subcommand).

The command grammar is `local-machine <subcommand> <action> [parameters]`:

- A **subcommand** names one capability (e.g. `export`) and is a group, not a single verb.
- An **action** is a verb under that subcommand (e.g. `start`, `status`, `stop`). The
  operations on a capability MUST be actions of its one subcommand, not sibling
  subcommands (no `unexport`, `export-status`) and not mode-selecting flags
  (no `export --stop`).
- Actions that mean the same thing across subcommands MUST use the same verb
  (e.g. every subcommand with a lifecycle uses `start` / `status` / `stop`).
- Every feature spec MUST name its subcommand and list its actions.

Rationale: one command makes the full toolset discoverable without remembering where each
tool lives.

### II. Configuration, Not Hardcoding (YAML)

Machine-specific facts — IP addresses, hostnames, paths, credentials references, and any
other value that describes this particular virtual machine — MUST live in configuration
files, never in source code. Configuration files MUST be YAML. Code MAY ship defaults only
for values that are not machine-specific.

Configuration MUST be stored in a configuration directory whose default is
`~/.config/local-machine`. The location is a parameter, not a constant: every subcommand
that reads configuration MUST accept the same `--config-dir` parameter (Principle III) to
override the default, and MUST NOT read configuration from any other location.

Rationale: the tools describe *how* to manage a machine; the config describes *which*
machine. Keeping them apart lets the same binary manage a rebuilt or replacement VM by
editing YAML alone.

### III. Shared Parameter Vocabulary

When two or more subcommands take the same logical parameter, they MUST use the same
parameter name (same long flag, same short flag if any, same meaning, same value format).
A new subcommand MUST reuse an existing parameter name before inventing a new one; a new
name is permitted only for a concept no existing parameter covers.

Rationale: users learn a flag once and it works everywhere; divergent names for one concept
are a discoverability and correctness hazard.

### IV. Rust Implementation

The `local-machine` tool and all of its subcommands MUST be written in Rust. Shell or other
languages MUST NOT be used to implement subcommand logic; invoking external system
programs from Rust is permitted where that is the natural way to manage the system.

Rationale: a single implementation language keeps the toolset one coherent codebase with
one build and one dependency story.

### V. Ubuntu Linux Target

The tools MUST assume an Ubuntu Linux system (e.g. `apt`, `systemd`, Ubuntu filesystem
layout). Portability to other distributions or operating systems is not a goal and MUST NOT
add complexity to the code.

Rationale: the tool manages one known kind of machine; designing for others is cost without
benefit.

## Scope & Machine Boundary

This project contains scripts and tools for managing this particular virtual machine (see
`~/src/CLAUDE.md` for the surrounding workspace). Features MUST serve management of that
machine. The identity of the machine is supplied by YAML configuration (Principle II), not
by assumptions in code.

## Source Control

The project's source lives in a local git repository. Changes are made and committed
locally; no remote hosting is assumed by this constitution.

## Governance

- This constitution is derived from the project's `CLAUDE.md`. The two MUST stay
  consistent: an amendment to either requires updating the other in the same change.
- This constitution supersedes conflicting practice. Specs, plans, and tasks produced by
  Spec Kit MUST be checked against Principles I–V; any deviation MUST be justified
  explicitly in the plan's complexity tracking.
- Amendments are made via `/speckit-constitution`, recorded with a Sync Impact Report, and
  versioned semantically: MAJOR for removed or redefined principles, MINOR for added
  principles or sections or materially expanded guidance, PATCH for clarifications.
- Every review of a new subcommand MUST verify: it hangs off `local-machine` and follows the
  `<subcommand> <action>` grammar (I), contains no
  machine-specific literals and reads config only from `--config-dir` (II), reuses existing parameter names (III), is Rust (IV), and
  targets Ubuntu only (V).

**Version**: 1.2.0 | **Ratified**: 2026-10-30 | **Last Amended**: 2026-10-30
